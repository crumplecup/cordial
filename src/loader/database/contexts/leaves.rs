//! The leaf pass: integration-test binaries read off the library.
//!
//! A test binary consumes the library and nothing consumes it. Its `#[test]`
//! functions are active only under the `test` cfg, so the pass reads each leaf
//! function's calls from the context's copied crates. The ordinary build's
//! inputs stay unchanged. Nothing here adds a node to the library's graph.

use super::LocalCfgs;
use super::copy_functions::copy_functions;
use super::flips::{Flips, Reach};
use super::pass::functions_by_range;
use super::plan::{Covered, asked};
use crate::loader::database::{
    SourceDatabase,
    item_key::ItemKey,
    populate::expanded_definition,
    queries::{evidence_of, persisted_evidence},
};
use crate::{CordialResult, CrateIr, FunctionLocation, LeafFunction, SemanticContextKey};
use ra_ap_cfg::CfgOptions;
use ra_ap_hir::{Semantics, attach_db};
use ra_ap_syntax::{AstNode, TextRange, ast, ast::HasName};
use std::collections::{BTreeSet, VecDeque};
use std::path::PathBuf;

use tracing::instrument;

/// What a leaf context after the first reads of the functions it has: the ones a gate it flips (from
/// the first's cfg) can change, and those the contexts before it did not have.
///
/// A function with no flipped gate in it, around it, or in a name it mentions has the same facts
/// as it had where it was first read, and joining them again adds nothing.
struct Flipped<'a> {
    /// Each local crate's cfg in the first context beside the one in this.
    pairs: Vec<(CfgOptions, CfgOptions)>,
    /// The functions the contexts before this read.
    known: &'a BTreeSet<FunctionLocation>,
}

/// Where the loop over the leaf files' contexts has got to, as another process can take it up from.
#[derive(serde::Serialize, serde::Deserialize)]
struct LeavesState {
    leaves: Vec<LeafFunction>,
    seen: Vec<SemanticContextKey>,
    queue: Vec<SemanticContextKey>,
    covered: Vec<(String, super::plan::Clause)>,
}

impl SourceDatabase {
    /// Read the crate's observability inventory again in the cfg context the
    /// database is in and add it to the one the ordinary build produced, so a
    /// print or event in code only this context activates is seen.
    #[instrument(level = "debug", skip_all, err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(in crate::loader::database) fn merge_context_observability(
        &self,
        graph: &mut CrateIr,
        context: &crate::ContextCrates,
    ) -> CordialResult<()> {
        use crate::IrView;
        let mut inventory = graph.source_observability()?;
        let seen = self.project_observability(inventory.crate_root(), Some(context))?;
        inventory.merge_context(seen);
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_OBSERVABILITY,
            serde_json::to_value(inventory)?,
        )
    }

    /// Record, under `ATTR_LEAF_FUNCTIONS`, what each function of the crate's
    /// integration-test files calls.
    #[instrument(level = "debug", skip_all, err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(in crate::loader::database) fn project_leaves(
        &mut self,
        graph: &mut CrateIr,
    ) -> CordialResult<()> {
        use crate::IrView;
        let files: Vec<PathBuf> = graph
            .source_observability()?
            .files()
            .iter()
            .filter(|file| file.kind() == crate::ir::ObservabilityFileKind::Test)
            .map(|file| file.snapshot().path().clone())
            .collect();
        let mut leaves: Vec<LeafFunction> = Vec::new();
        if !files.is_empty() {
            let read = self.read_leaf_contexts(&files, &mut leaves);
            self.release_shared_copies("leaf contexts");
            read?;
        }
        graph.set_attr(
            graph.root(),
            crate::ATTR_LEAF_FUNCTIONS,
            serde_json::to_value(leaves)?,
        )
    }

    /// Read the leaf files under the `test` cfg and under every context their own
    /// gates ask for. A leaf function gated by a feature or another cfg is active
    /// only where that gate holds, and the gates are taken one clause at a time:
    /// merging them, as the library's planning does, can set one atom both ways
    /// for a file that needs it on. Repeats until no gate asks for more.
    #[instrument(level = "debug", skip(self, files, leaves), err(level = "warn"))]
    #[hotpath::measure]
    fn read_leaf_contexts(
        &mut self,
        files: &[PathBuf],
        leaves: &mut Vec<LeafFunction>,
    ) -> CordialResult<()> {
        let mut only = BTreeSet::new();
        for path in files {
            only.insert(self.file_id(path)?);
        }
        let base = SemanticContextKey::enabling(["test"]);
        let cfgs = LocalCfgs::capture(&self.db, &self.analysed_crates());
        let in_base: Vec<CfgOptions> = cfgs.pairs(&base).into_iter().map(|(_, cfg)| cfg).collect();
        let mut covered = Covered::default();
        let mut seen = vec![base.clone()];
        let mut queue = VecDeque::from([base.clone()]);
        // A process that takes up a run goes on from the leaf contexts an earlier one read.
        if let Some(state) = self
            .leaves_progress()
            .and_then(|bytes| serde_json::from_slice::<LeavesState>(&bytes).ok())
        {
            tracing::info!(
                target: "cordial::semantic::leaves",
                read = state.seen.len() - state.queue.len(),
                queued = state.queue.len(),
                "the leaf contexts are taken up from an earlier process"
            );
            *leaves = state.leaves;
            seen = state.seen;
            queue = state.queue.into_iter().collect();
            covered = Covered::from_paths(state.covered, &|path| {
                self.file_id(std::path::Path::new(path)).ok()
            });
        }
        let mut outcome = Ok(());
        let flips = self.flips_contexts();
        let mut run = self.begin_flips(&cfgs);
        let mut current = SemanticContextKey::ordinary();
        while let Some(context) = next_leaf_context(&mut queue, flips.then_some(&current)) {
            current = context.clone();
            let _leaf = crate::Segment::begin("leaf");
            let stage = std::time::Instant::now();
            let enter = crate::Segment::begin("enter");
            let added = match if flips {
                Ok(self.flip_context(&cfgs, &context))
            } else {
                self.add_context_crates(&context)
            } {
                Ok(added) => added,
                Err(error) => {
                    self.evict_regenerable("failed leaf context registration");
                    outcome = Err(error);
                    break;
                }
            };
            let entered = stage.elapsed();
            drop(enter);
            let stage = std::time::Instant::now();
            let read = crate::Segment::begin("read");
            let mut found = Vec::new();
            let known: BTreeSet<FunctionLocation> =
                leaves.iter().map(|leaf| leaf.location().clone()).collect();
            let flipped = (context != base).then(|| Flipped {
                pairs: in_base
                    .iter()
                    .cloned()
                    .zip(cfgs.pairs(&context).into_iter().map(|(_, cfg)| cfg))
                    .collect(),
                known: &known,
            });
            outcome = self.read_leaves(files, &added, flipped.as_ref(), &mut found);
            drop(read);
            tracing::info!(
                target: "cordial::semantic::leaves",
                %context,
                found = found.len(),
                entered_ms = entered.as_millis() as u64,
                read_ms = stage.elapsed().as_millis() as u64,
                "leaf context read"
            );
            merge(leaves, found);
            if outcome.is_err() {
                self.end_context(&added, "failed leaf context", &cfgs, run.as_mut());
                break;
            }
            for next in asked(
                &self.db,
                &context,
                &mut covered,
                Some(&only),
                &added.crate_set(),
                &added.originals(),
            ) {
                if !seen.contains(&next) {
                    seen.push(next.clone());
                    queue.push_back(next);
                }
            }
            // Planning and persistence preparation finish before eviction.
            let end = crate::Segment::begin("end");
            self.end_context(&added, "leaf context", &cfgs, run.as_mut());
            drop(end);
            // What a process that takes up the run needs to go on from here is kept, and the
            // process ends here if it is the kind that does.
            let _kept = crate::Segment::begin("kept");
            let state = LeavesState {
                leaves: leaves.clone(),
                seen: seen.clone(),
                queue: queue.iter().cloned().collect(),
                covered: covered.by_path(&|file| Some(self.vfs.file_path(file).to_string())),
            };
            if let Ok(bytes) = serde_json::to_vec(&state) {
                self.keep_progress(
                    "leaves",
                    &bytes,
                    &format!("leaf-context-{}", seen.len() - queue.len()),
                );
            }
        }
        if let Some(run) = run {
            self.finish_flips(&cfgs, run);
        }
        outcome
    }

    #[instrument(level = "debug", skip(self, files, only, leaves), err(level = "warn"))]
    #[hotpath::measure]
    fn read_leaves(
        &self,
        files: &[PathBuf],
        context: &crate::ContextCrates,
        only: Option<&Flipped<'_>>,
        leaves: &mut Vec<LeafFunction>,
    ) -> CordialResult<()> {
        attach_db(&self.db, || {
            let sema = Semantics::new(&self.db);
            let roots = self
                .projection_context()?
                .packages()
                .iter()
                .map(|package| package.root().clone())
                .collect();
            let locator = super::super::external::Locator::new(&self.db, roots);
            // The functions of each copy as the last run saved them, when every one of them can
            // be found again from its description: their evidence is the saved evidence, and
            // nothing is asked of rust-analyzer.
            let mut saved = Vec::new();
            let mut every = true;
            for path in files {
                let Some(file) = context.file_for(self.file_id(path)?) else {
                    continue;
                };
                // The functions of a copy were listed when it was saved; flipped crates have no copy to list.
                let listed = if context.is_in_place() { None } else { copy_functions(&self.db, file) };
                match listed {
                    Some(records) if records.iter().all(|record| *record.found()) => {
                        saved.push((path, records));
                    }
                    _ => {
                        every = false;
                        break;
                    }
                }
            }
            if every {
                // The functions to read: all of them in the first context, and in the others those
                // a flipped gate can change or the earlier contexts did not have.
                let mut chosen = Vec::new();
                let mut listed = 0usize;
                for (path, records) in saved {
                    listed += records.len();
                    let kept: Vec<_> = match only {
                        None => records.iter().collect(),
                        Some(only) => {
                            let original = self.file_id(path)?;
                            let flips =
                                Flips::of_file(&self.db, original, &only.pairs, Reach::Seen);
                            let items = std::cell::OnceCell::new();
                            records
                                .iter()
                                .filter(|record| {
                                    let range = TextRange::new(
                                        (*record.start()).into(),
                                        (*record.end()).into(),
                                    );
                                    !only.known.contains(&FunctionLocation::new(
                                        path.clone(),
                                        *record.start(),
                                    )) || flips.touches(range)
                                        || (flips.has_names()
                                            && items
                                                .get_or_init(|| {
                                                    functions_by_range(&self.db, original)
                                                })
                                                .get(&range)
                                                .is_some_and(|function| {
                                                    flips.mentions(function.syntax())
                                                }))
                                })
                                .collect()
                        }
                    };
                    chosen.push((path, kept));
                }
                tracing::info!(
                    target: "cordial::semantic::leaves",
                    functions = listed,
                    read = chosen.iter().map(|(_, kept)| kept.len()).sum::<usize>(),
                    first_context = only.is_none(),
                    "the functions of the leaf files a context reads"
                );
                let keys: Vec<String> = chosen
                    .iter()
                    .flat_map(|(_, records)| records.iter().map(|record| record.key().clone()))
                    .collect();
                self.prime("leaf files", &keys, |db, text| {
                    let _ = persisted_evidence(db, ItemKey::new(db, text.clone()));
                });
                for (path, records) in chosen {
                    for record in records {
                        let key = ItemKey::new(&self.db, record.key().clone());
                        let Some(facts) = persisted_evidence(&self.db, key).as_ref() else {
                            continue;
                        };
                        let callees = facts
                            .calls()
                            .iter()
                            .filter_map(|call| call.callee().clone())
                            .collect();
                        let called = facts
                            .calls()
                            .iter()
                            .filter_map(|call| match call.target() {
                                crate::SemanticCallTarget::Function(id) => locator.locate(*id),
                                _ => None,
                            })
                            .collect();
                        leaves.push(LeafFunction::new(
                            FunctionLocation::new(path.clone(), *record.start()),
                            callees,
                            called,
                        ));
                    }
                }
                return Ok(());
            }
            tracing::info!(
                target: "cordial::semantic::fallback",
                step = "leaves",
                "a context's copy has a function a description cannot find again: its functions are read through rust-analyzer"
            );
            // Each context is a new cfg, so none of the results are computed yet:
            // evaluate the leaf functions together, then read them below.
            let mut definitions = Vec::new();
            for path in files {
                let Some(file) = context.file_for(self.file_id(path)?) else {
                    continue;
                };
                let tree = sema.parse(sema.attach_first_edition(file));
                definitions.extend(
                    tree.syntax()
                        .descendants()
                        .filter_map(ast::Fn::cast)
                        .filter_map(|function| {
                            sema.to_def(&function)
                                .or_else(|| expanded_definition(&sema, &function))
                        }),
                );
            }
            self.prime_functions("leaf files", &definitions);
            for path in files {
                let Some(file) = context.file_for(self.file_id(path)?) else {
                    continue;
                };
                let tree = sema.parse(sema.attach_first_edition(file));
                for function in tree.syntax().descendants().filter_map(ast::Fn::cast) {
                    let Some(definition) = sema
                        .to_def(&function)
                        .or_else(|| expanded_definition(&sema, &function))
                    else {
                        tracing::debug!(
                            target: "cordial::semantic::leaves",
                            file = %path.display(),
                            function = function.name().map(|name| name.to_string()).unwrap_or_default(),
                            "leaf function with no definition under the test cfg"
                        );
                        continue;
                    };
                    let Some(facts) = evidence_of(&self.db, definition).as_ref() else {
                        continue;
                    };
                    let callees = facts
                        .calls()
                        .iter()
                        .filter_map(|call| call.callee().clone())
                        .collect();
                    let called = facts
                        .calls()
                        .iter()
                        .filter_map(|call| match call.target() {
                            crate::SemanticCallTarget::Function(id) => locator.locate(*id),
                            _ => None,
                        })
                        .collect();
                    let start = u32::from(function.syntax().text_range().start());
                    leaves.push(LeafFunction::new(
                        FunctionLocation::new(path.clone(), start),
                        callees,
                        called,
                    ));
                }
            }
            Ok(())
        })
    }
}

/// The next leaf context: the one fewest atoms from `current` when the crates are flipped, else the first.
fn next_leaf_context(
    queue: &mut VecDeque<SemanticContextKey>,
    current: Option<&SemanticContextKey>,
) -> Option<SemanticContextKey> {
    let Some(current) = current else {
        return queue.pop_front();
    };
    let nearest = queue
        .iter()
        .enumerate()
        .min_by_key(|(_, context)| current.atoms_apart(context))
        .map(|(index, _)| index)?;
    queue.remove(nearest)
}

/// Add `found` to `leaves`, joining a function seen in more than one context.
#[instrument(level = "trace", skip_all)]
fn merge(leaves: &mut Vec<LeafFunction>, found: Vec<LeafFunction>) {
    for leaf in found {
        match leaves
            .iter_mut()
            .find(|known| known.location() == leaf.location())
        {
            Some(known) => {
                let mut callees = known.callees().clone();
                let mut called = known.called().clone();
                for callee in leaf.callees() {
                    if !callees.contains(callee) {
                        callees.push(callee.clone());
                    }
                }
                for location in leaf.called() {
                    if !called.contains(location) {
                        called.push(location.clone());
                    }
                }
                *known = LeafFunction::new(known.location().clone(), callees, called);
            }
            None => leaves.push(leaf),
        }
    }
}
