//! Harvest each cfg context's facts into the IR.
//!
//! Each context is a copy of the workspace's crates under its cfg, added beside
//! the ordinary ones (see `context_crates`). Nothing is toggled: the ordinary
//! crates' inputs are never written, so what the database computed for them
//! stays valid, and a context's facts are read from its own crates and copied
//! into the IR.

use super::LocalCfgs;
use super::copy_functions::copy_functions;
use super::flips::{Flips, Reach};
use super::merge::{Pending, declares_a_name_twice, group};
use super::plan::{Covered, asked};
use crate::loader::database::{
    SourceDatabase,
    edition::edition_of,
    item_key::{self, ItemKey},
    populate::expanded_definition,
    queries::{evidence_of, persisted_evidence},
    types,
};
use crate::{
    CfgPlanning, CordialResult, CrateIr, NodeId, SemanticContextFunction, SemanticContextKey,
    SemanticContextPlan, SemanticItemId,
};
use ra_ap_cfg::CfgOptions;
use ra_ap_hir::{Semantics, attach_db};
use ra_ap_syntax::{AstNode, TextRange, ast};
use salsa::plumbing::AsId;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use tracing::instrument;

/// Where the pass over the cfg contexts has got to, in a form another process can take it up from:
/// what is analysed, what is queued, what is requested and what the planning has covered (by file
/// path, since a file's number is its process's own).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(in crate::loader::database) struct ContextsState {
    analysed: Vec<SemanticContextKey>,
    seen: Vec<SemanticContextKey>,
    queue: Vec<(SemanticContextKey, Vec<SemanticContextKey>)>,
    covered: Vec<(String, super::plan::Clause)>,
}

impl ContextsState {
    /// How many contexts are analysed.
    #[must_use]
    pub(in crate::loader::database) fn analysed(&self) -> usize {
        self.analysed.len()
    }
}

/// Contexts still to analyse, and every request ever made.
#[derive(Debug, Default)]
struct Wanted {
    seen: Vec<SemanticContextKey>,
    queue: VecDeque<Pending>,
}

impl Wanted {
    /// Queue contexts the caller named, each analysed exactly as written.
    #[instrument(level = "trace", skip(self, contexts))]
    fn add_exact(&mut self, contexts: Vec<SemanticContextKey>) {
        for context in contexts {
            if !context.is_ordinary() && !self.seen.contains(&context) {
                self.seen.push(context.clone());
                self.queue.push_back(Pending::single(context));
            }
        }
    }

    /// Queue the requests not seen before, combining the compatible ones.
    #[instrument(level = "trace", skip(self, contexts))]
    fn add(&mut self, contexts: Vec<SemanticContextKey>) {
        let mut fresh = Vec::new();
        for context in contexts {
            if !context.is_ordinary() && !self.seen.contains(&context) {
                self.seen.push(context.clone());
                fresh.push(context);
            }
        }
        for pending in group(fresh) {
            self.seen.push(pending.key().clone());
            self.queue.push_back(pending);
        }
    }
}

/// The graph node of every function item, by file and source range.
#[derive(Debug, Default)]
pub(in crate::loader::database) struct FunctionNodes {
    nodes: HashMap<(PathBuf, TextRange), NodeId>,
    by_file: HashMap<PathBuf, Vec<NodeId>>,
}

/// What a [`FunctionNodes`] is made of, in a form another process can read: ranges as their two ends.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(in crate::loader::database) struct FunctionNodesState {
    nodes: Vec<(PathBuf, u32, u32, NodeId)>,
    by_file: Vec<(PathBuf, Vec<NodeId>)>,
}

impl FunctionNodes {
    /// This, in the form a store keeps it in.
    #[instrument(level = "trace", skip(self))]
    pub(in crate::loader::database) fn to_state(&self) -> FunctionNodesState {
        FunctionNodesState {
            nodes: self
                .nodes
                .iter()
                .map(|((path, range), node)| {
                    (
                        path.clone(),
                        u32::from(range.start()),
                        u32::from(range.end()),
                        *node,
                    )
                })
                .collect(),
            by_file: self
                .by_file
                .iter()
                .map(|(path, nodes)| (path.clone(), nodes.clone()))
                .collect(),
        }
    }

    /// What a store kept, as it was.
    #[instrument(level = "trace", skip(state))]
    pub(in crate::loader::database) fn from_state(state: FunctionNodesState) -> Self {
        Self {
            nodes: state
                .nodes
                .into_iter()
                .map(|(path, start, end, node)| {
                    ((path, TextRange::new(start.into(), end.into())), node)
                })
                .collect(),
            by_file: state.by_file.into_iter().collect(),
        }
    }

    #[instrument(level = "trace", skip(self, path, range, node))]
    pub(in crate::loader::database) fn insert(
        &mut self,
        path: &Path,
        range: TextRange,
        node: NodeId,
    ) {
        if self
            .nodes
            .insert((path.to_path_buf(), range), node)
            .is_none()
        {
            self.by_file
                .entry(path.to_path_buf())
                .or_default()
                .push(node);
        }
    }

    /// The nodes of the functions in the file at `path`.
    #[instrument(level = "trace", skip(self, path))]
    pub(in crate::loader::database) fn in_file(&self, path: &Path) -> &[NodeId] {
        self.by_file.get(path).map_or(&[], Vec::as_slice)
    }

    #[instrument(level = "trace", skip(self, path, range))]
    pub(in crate::loader::database) fn get(&self, path: &Path, range: TextRange) -> Option<NodeId> {
        self.nodes.get(&(path.to_path_buf(), range)).copied()
    }
}

impl SourceDatabase {
    /// Analyse the cfg contexts the options name and, when planning is on, the
    /// ones the workspace's own gates ask for, recording their facts in `graph`.
    #[instrument(level = "debug", skip_all, err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(in crate::loader::database) fn project_contexts(
        &mut self,
        paths: &[PathBuf],
        nodes: &FunctionNodes,
        graph: &mut CrateIr,
    ) -> CordialResult<()> {
        let cfgs = LocalCfgs::capture(&self.db, &self.analysed_crates());
        let mut analysed = Vec::new();
        // A process that takes up a run an earlier one began goes on from the contexts it analysed.
        let resume = self.resume_contexts.take();
        let outcome = self.analyse_contexts(&cfgs, paths, nodes, graph, &mut analysed, resume);
        // Gated code is extra evidence: failing to analyse it must not fail a run
        // that asked for nothing else. The plan says it is incomplete, and the log
        // carries the whole error chain.
        let complete = outcome.unwrap_or_else(|error| {
            tracing::warn!(
                target: "cordial::semantic::contexts",
                error = ?error,
                analysed = analysed.len(),
                "cfg context analysis failed; gated code beyond it reports no active context"
            );
            false
        });
        let plan = SemanticContextPlan::new(analysed, complete);
        graph.set_attr(
            graph.root(),
            crate::ATTR_SEMANTIC_CONTEXT_PLAN,
            serde_json::to_value(plan)?,
        )?;
        Ok(())
    }

    /// Analyse queued contexts, recording each in `analysed` as it starts so the
    /// plan is accurate even when an error ends the pass. Returns whether planning
    /// reached its fixed point.
    #[instrument(level = "debug", skip_all, err(level = "warn"))]
    #[hotpath::measure]
    fn analyse_contexts(
        &mut self,
        cfgs: &LocalCfgs,
        paths: &[PathBuf],
        nodes: &FunctionNodes,
        graph: &mut CrateIr,
        analysed: &mut Vec<SemanticContextKey>,
        resume: Option<ContextsState>,
    ) -> CordialResult<bool> {
        let options = self.cargo_options.clone().unwrap_or_default();
        let automatic = *options.cfg_planning() == CfgPlanning::Automatic;
        let ordinary = SemanticContextKey::ordinary();
        let mut covered = Covered::default();
        let mut wanted = Wanted::default();
        if let Some(state) = resume {
            // The planning was done by the process that began this: what it asked for and had
            // covered is what this one goes on from.
            tracing::info!(
                target: "cordial::semantic::contexts",
                analysed = state.analysed.len(),
                queued = state.queue.len(),
                "the cfg contexts are taken up from an earlier process"
            );
            analysed.extend(state.analysed);
            wanted.seen = state.seen;
            wanted.queue = state
                .queue
                .into_iter()
                .map(|(key, members)| Pending::restored(key, members))
                .collect();
            covered =
                Covered::from_paths(state.covered, &|path| self.file_id(Path::new(path)).ok());
        } else {
            self.reporter.say("Planning cfg contexts");
            wanted.add_exact(options.cfg_contexts().clone());
        }
        if automatic && !self.has_resumed_contexts(analysed, &wanted) {
            let _watch = crate::process_memory::Watch::start("planning scan".to_owned());
            wanted.add(asked(
                &self.db,
                &ordinary,
                &mut covered,
                None,
                &self.analysed_crates(),
                &HashMap::new(),
            ));
            crate::process_memory::log("planning scan done");
        }
        while let Some(pending) = wanted.queue.pop_front() {
            let context = pending.key().clone();
            self.reporter.say(format!(
                "Analysing cfg context {} ({} queued): {context}",
                analysed.len() + 1,
                wanted.queue.len()
            ));
            let watch =
                crate::process_memory::Watch::start(format!("cfg context {}", analysed.len() + 1));
            let stage = std::time::Instant::now();
            let added = match self.add_context_crates(&context) {
                Ok(added) => added,
                Err(error) => {
                    self.evict_regenerable("failed cfg context registration");
                    return Err(error);
                }
            };
            let entered = stage.elapsed();
            crate::process_memory::log("cfg context entered");
            if pending.is_merged() && declares_a_name_twice(&self.db, &added.crate_set()) {
                tracing::debug!(
                    target: "cordial::semantic::contexts",
                    %context,
                    members = pending.members().len(),
                    "merged cfg context declares a name twice; splitting it"
                );
                for half in pending.halves().into_iter().rev() {
                    wanted.queue.push_front(half);
                }
                self.finish_context(&added, "split cfg context");
                continue;
            }
            let index = u32::try_from(analysed.len()).unwrap_or(u32::MAX);
            analysed.push(context.clone());
            let pairs = cfgs.pairs(&context);
            let files = Files {
                paths,
                nodes,
                context: &added,
            };
            let stage = std::time::Instant::now();
            let outcome = self.harvest(&context, index, &pairs, &files, graph);
            let harvested = stage.elapsed();
            crate::process_memory::log("cfg context harvested");
            let outcome = outcome.and_then(|()| {
                let stage = std::time::Instant::now();
                #[cfg(feature = "tracing")]
                self.merge_context_observability(graph, &added)?;
                let merged = stage.elapsed();
                let stage = std::time::Instant::now();
                if automatic {
                    wanted.add(asked(
                        &self.db,
                        &context,
                        &mut covered,
                        None,
                        &added.crate_set(),
                        &added.originals(),
                    ));
                }
                Ok((merged, stage.elapsed()))
            });
            // Observability and gate discovery still read this context after harvesting.
            // Ask for eviction only once those consumers have finished.
            let stage = std::time::Instant::now();
            self.finish_context(&added, "cfg context");
            if crate::memory_limit::pressured() {
                graph.drop_decoded_attrs();
            }
            let finished = stage.elapsed();
            drop(watch);
            let (merged, planned) = outcome?;
            // What a process that takes up the run needs to go on from here is kept, and the process
            // ends here if it is the kind that does.
            let state = self.contexts_state(analysed, &wanted, &covered);
            self.stage_done(graph, Some(&state), &format!("context-{}", analysed.len()));
            tracing::info!(
                target: "cordial::semantic::contexts",
                %context,
                analysed = analysed.len(),
                queued = wanted.queue.len(),
                entered_ms = entered.as_millis() as u64,
                harvested_ms = harvested.as_millis() as u64,
                finished_ms = finished.as_millis() as u64,
                observability_ms = merged.as_millis() as u64,
                planned_ms = planned.as_millis() as u64,
                "cfg context analysed"
            );
            crate::process_memory::log("cfg context analysed");
        }
        Ok(wanted.queue.is_empty())
    }

    /// Whether the planning scan was done already, by a process this one takes the run up from.
    fn has_resumed_contexts(&self, analysed: &[SemanticContextKey], wanted: &Wanted) -> bool {
        !analysed.is_empty() || !wanted.queue.is_empty() || !wanted.seen.is_empty()
    }

    /// Where the pass has got to, as another process can take it up from.
    #[instrument(level = "trace", skip_all)]
    fn contexts_state(
        &self,
        analysed: &[SemanticContextKey],
        wanted: &Wanted,
        covered: &Covered,
    ) -> ContextsState {
        ContextsState {
            analysed: analysed.to_vec(),
            seen: wanted.seen.clone(),
            queue: wanted
                .queue
                .iter()
                .map(|pending| (pending.key().clone(), pending.members().clone()))
                .collect(),
            covered: covered.by_path(&|file| Some(self.vfs.file_path(file).to_string())),
        }
    }

    /// Copy the active context's facts into `graph`.
    ///
    /// Every function that resolves records that it is active in this context.
    /// Full facts and call edges are kept only where this context's cfg change
    /// can matter: a function inactive in the ordinary build, one inside or
    /// holding a gate the context flips, or one that mentions a name such a gate
    /// defines.
    #[instrument(level = "debug", skip_all, fields(context = %context), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn harvest(
        &self,
        context: &SemanticContextKey,
        index: u32,
        pairs: &[(CfgOptions, CfgOptions)],
        files: &Files<'_>,
        graph: &mut CrateIr,
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
            // An item of this context's copy is named as the ordinary build names it
            // when the ordinary build has it, so facts and lookups agree.
            let canonical_id =
                |id: SemanticItemId| -> SemanticItemId { item_key::canonical_id(&self.db, id) };
            let mut definitions: HashMap<SemanticItemId, NodeId> = HashMap::new();
            // Each call as the facts give its target, as the context's copy names it (which is what
            // can be located) and as the ordinary build names it (which is what is recorded).
            let mut calls: Vec<(NodeId, SemanticItemId, SemanticItemId)> = Vec::new();
            let (mut saved_files, mut rust_analyzer_files, mut aliased_files) =
                (0usize, 0usize, 0usize);
            // The functions the context's copy has, and those of them it kept full facts for
            // (the rest are only marked active): the share a context changes.
            let (mut functions, mut kept, mut new_here) = (0usize, 0usize, 0usize);
            for path in files.paths {
                let original = self.file_id(path)?;
                if files.context.is_aliased(original) {
                    // The context takes this file as it is: its functions are the ordinary
                    // build's, active here as there, and nothing in the file is switched.
                    aliased_files += 1;
                    for node in files.nodes.in_file(path) {
                        push_activity(graph, *node, index)?;
                        functions += 1;
                        if let Some(function) = crate::IrView::semantic_function(graph, *node)? {
                            definitions.insert(*function.definition(), *node);
                        }
                    }
                    continue;
                }
                // The context's copy of the file, which has the same text.
                let Some(file) = files.context.copy_of(original) else {
                    continue;
                };
                // The functions of the copy as the last run saved them, when every one of them can
                // be found again from its description: nothing is asked of rust-analyzer.
                let listed = copy_functions(&self.db, file);
                if let Some(records) =
                    listed.filter(|records| records.iter().all(|record| *record.found()))
                {
                    saved_files += 1;
                    // The copy has the original's text: its gates are read once, from the original,
                    // and its syntax is the original's, which is parsed only if a function asks.
                    let flips = Flips::of_file(&self.db, original, pairs, Reach::Written);
                    let items = std::cell::OnceCell::new();
                    for record in records {
                        let range =
                            TextRange::new((*record.start()).into(), (*record.end()).into());
                        let Some(node) = files.nodes.get(path, range) else {
                            continue;
                        };
                        let key = ItemKey::new(&self.db, record.key().clone());
                        let own = SemanticItemId::new(key.as_id().as_bits());
                        definitions.insert(canonical_id(own), node);
                        push_activity(graph, node, index)?;
                        functions += 1;
                        let new = crate::IrView::semantic_function(graph, node)?.is_none();
                        if new {
                            new_here += 1;
                            locator.record_trait_implementation(graph, own, node)?;
                        }
                        let relevant = new
                            || flips.touches(range)
                            || (flips.has_names()
                                && items
                                    .get_or_init(|| functions_by_range(&self.db, original))
                                    .get(&range)
                                    .is_some_and(|function| flips.mentions(function.syntax())));
                        if !relevant {
                            continue;
                        }
                        kept += 1;
                        let facts =
                            persisted_evidence(&self.db, key).as_ref().ok_or_else(|| {
                                crate::CordialError::workspace_source_facts_missing(
                                    "semantic function source",
                                )
                            })?;
                        let facts = match record.anchor() {
                            Some(anchor) => facts.with_ranges_from(*anchor as usize),
                            None => facts.clone(),
                        };
                        for call in facts.calls() {
                            if let crate::SemanticCallTarget::Function(target) = call.target() {
                                calls.push((node, canonical_id(*target), *target));
                            }
                        }
                        let facts = facts.map_identities(&mut |id| canonical_id(id));
                        push_facts(
                            graph,
                            node,
                            SemanticContextFunction::new(context.clone(), facts),
                        )?;
                    }
                    continue;
                }
                tracing::info!(
                    target: "cordial::semantic::fallback",
                    step = "harvest",
                    %context,
                    file = %path.display(),
                    listed = listed.is_some(),
                    unfound = listed.map_or(0, |records| records.iter().filter(|record| !*record.found()).count()),
                    "a copy's functions are read through rust-analyzer"
                );
                rust_analyzer_files += 1;
                let tree = sema.parse(sema.attach_first_edition(file));
                let flips = Flips::collect(&tree, pairs, Reach::Written);
                for function in tree.syntax().descendants().filter_map(ast::Fn::cast) {
                    let range = function.syntax().text_range();
                    let Some(node) = files.nodes.get(path, range) else {
                        continue;
                    };
                    let Some(definition) = sema
                        .to_def(&function)
                        .or_else(|| expanded_definition(&sema, &function))
                    else {
                        continue;
                    };
                    definitions.insert(
                        canonical_id(types::identity(&self.db, definition.into())),
                        node,
                    );
                    push_activity(graph, node, index)?;
                    functions += 1;
                    let new = crate::IrView::semantic_function(graph, node)?.is_none();
                    if new {
                        new_here += 1;
                        locator.record_trait_implementation(
                            graph,
                            types::identity(&self.db, definition.into()),
                            node,
                        )?;
                    }
                    if !(new || flips.touches(range) || flips.mentions(function.syntax())) {
                        continue;
                    }
                    kept += 1;
                    let facts = evidence_of(&self.db, definition).as_ref().ok_or_else(|| {
                        crate::CordialError::workspace_source_facts_missing(
                            "semantic function source",
                        )
                    })?;
                    let facts = super::super::queries::in_file(&self.db, definition, facts);
                    for call in facts.calls() {
                        if let crate::SemanticCallTarget::Function(target) = call.target() {
                            calls.push((node, canonical_id(*target), *target));
                        }
                    }
                    let facts = facts.map_identities(&mut |id| canonical_id(id));
                    push_facts(
                        graph,
                        node,
                        SemanticContextFunction::new(context.clone(), facts),
                    )?;
                }
            }
            tracing::info!(
                target: "cordial::semantic::contexts",
                %context,
                saved_files,
                rust_analyzer_files,
                aliased_files,
                functions,
                new_here,
                kept,
                "the files of a context's copy: read from saved lists, and through rust-analyzer"
            );
            let mut edges: BTreeSet<(NodeId, NodeId)> = BTreeSet::new();
            let mut external: BTreeMap<NodeId, BTreeSet<crate::FunctionLocation>> = BTreeMap::new();
            for (caller, target, named_here) in calls {
                let callee = definitions.get(&target);
                tracing::debug!(
                    target: "cordial::semantic::contexts",
                    ?caller,
                    ?target,
                    %context,
                    resolved = callee.is_some(),
                    "context call edge"
                );
                if let Some(callee) = callee {
                    edges.insert((caller, *callee));
                } else if let Some(location) = locator.locate(named_here) {
                    external.entry(caller).or_default().insert(location);
                }
            }
            for (caller, callee) in edges {
                graph.insert_context_call(caller, callee, context.clone())?;
            }
            for (caller, locations) in external {
                push_external_calls(graph, caller, locations, context)?;
            }
            Ok(())
        })
    }
}

/// The functions of `file`, by where each is: read from the file's parse.
#[instrument(level = "trace", skip(db))]
#[hotpath::measure]
pub(super) fn functions_by_range(
    db: &ra_ap_ide_db::RootDatabase,
    file: ra_ap_vfs::FileId,
) -> HashMap<TextRange, ast::Fn> {
    edition_of(db, file)
        .parse(db)
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .map(|function| (function.syntax().text_range(), function))
        .collect()
}

/// The files of one projection, the graph node of each function in them, and the
/// context's copy of the workspace they are read through.
struct Files<'a> {
    paths: &'a [PathBuf],
    nodes: &'a FunctionNodes,
    context: &'a crate::ContextCrates,
}

/// Record that `node` is active in the context at `index`.
#[instrument(level = "trace", skip(graph, node), err(level = "warn"))]
fn push_activity(graph: &mut CrateIr, node: NodeId, index: u32) -> CordialResult<()> {
    let mut active: Vec<u32> = crate::IrView::node(graph, node)
        .and_then(|node| node.attr(crate::ATTR_SEMANTIC_ACTIVITY))
        .map(<Vec<u32> as serde::Deserialize>::deserialize)
        .transpose()?
        .unwrap_or_default();
    active.push(index);
    graph.set_attr(
        node,
        crate::ATTR_SEMANTIC_ACTIVITY,
        serde_json::to_value(active)?,
    )
}

/// Add the calls `node` makes into other crates in this context to those already
/// recorded on it.
#[instrument(
    level = "trace",
    skip(graph, locations, node, context),
    err(level = "warn")
)]
fn push_external_calls(
    graph: &mut CrateIr,
    node: NodeId,
    locations: BTreeSet<crate::FunctionLocation>,
    context: &SemanticContextKey,
) -> CordialResult<()> {
    let mut calls = crate::IrView::external_calls(graph, node)?;
    calls.extend(
        locations
            .into_iter()
            .map(|location| crate::ExternalCall::new(location, Some(context.clone()))),
    );
    graph.set_attr(
        node,
        crate::ATTR_EXTERNAL_CALLS,
        serde_json::to_value(calls)?,
    )
}

/// Add one context's facts to those already recorded on `node`.
#[instrument(level = "trace", skip(graph, entry, node), err(level = "warn"))]
fn push_facts(
    graph: &mut CrateIr,
    node: NodeId,
    entry: SemanticContextFunction,
) -> CordialResult<()> {
    let mut entries = crate::IrView::semantic_context_functions(graph, node)?;
    entries.push(entry);
    graph.set_attr(
        node,
        crate::ATTR_SEMANTIC_CONTEXT_FUNCTIONS,
        serde_json::to_value(entries)?,
    )
}
