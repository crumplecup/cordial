//! Harvest each cfg context's facts into the IR.
//!
//! Contexts are visited one at a time. Entering one changes the workspace
//! crates' cfg input, so only that context's variant is resident; its facts are
//! copied into the IR before the next diff replaces them, and the ordinary cfg
//! is restored at the end whatever happens.

use super::LocalCfgs;
use super::flips::Flips;
use super::merge::{Pending, declares_a_name_twice, group};
use super::plan::{Covered, asked};
use crate::loader::database::{
    SourceDatabase,
    populate::expanded_definition,
    queries::{FunctionKey, function_evidence},
    types,
};
use crate::{
    CfgPlanning, CordialResult, CrateIr, NodeId, SemanticContextFunction, SemanticContextKey,
    SemanticContextPlan, SemanticItemId,
};
use ra_ap_cfg::CfgOptions;
use ra_ap_hir::{Semantics, attach_db};
use ra_ap_syntax::{AstNode, TextRange, ast};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use tracing::instrument;

/// Most contexts analysed for one projection before the rest are reported as a gap.
const CONTEXT_LIMIT: usize = 64;

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
pub(in crate::loader::database) struct FunctionNodes(HashMap<(PathBuf, TextRange), NodeId>);

impl FunctionNodes {
    #[instrument(level = "trace", skip(self, path, range, node))]
    pub(in crate::loader::database) fn insert(
        &mut self,
        path: &Path,
        range: TextRange,
        node: NodeId,
    ) {
        self.0.insert((path.to_path_buf(), range), node);
    }

    #[instrument(level = "trace", skip(self, path, range))]
    pub(in crate::loader::database) fn get(&self, path: &Path, range: TextRange) -> Option<NodeId> {
        self.0.get(&(path.to_path_buf(), range)).copied()
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
        let cfgs = LocalCfgs::capture(&self.db);
        let mut analysed = Vec::new();
        let outcome = self.analyse_contexts(&cfgs, paths, nodes, graph, &mut analysed);
        cfgs.restore(&mut self.db);
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
    fn analyse_contexts(
        &mut self,
        cfgs: &LocalCfgs,
        paths: &[PathBuf],
        nodes: &FunctionNodes,
        graph: &mut CrateIr,
        analysed: &mut Vec<SemanticContextKey>,
    ) -> CordialResult<bool> {
        let options = self.cargo_options.clone().unwrap_or_default();
        let automatic = *options.cfg_planning() == CfgPlanning::Automatic;
        let ordinary = SemanticContextKey::ordinary();
        self.reporter.say("Planning cfg contexts");
        let budget = Budget::start(&options);
        let mut covered = Covered::default();
        let mut wanted = Wanted::default();
        wanted.add_exact(options.cfg_contexts().clone());
        if automatic {
            let _watch = crate::process_memory::Watch::start("planning scan".to_owned());
            wanted.add(asked(&self.db, &ordinary, &mut covered));
            crate::process_memory::log("planning scan done");
        }
        while let Some(pending) = wanted.queue.pop_front() {
            let context = pending.key().clone();
            if analysed.len() >= CONTEXT_LIMIT || budget.exceeded() {
                tracing::warn!(
                    target: "cordial::semantic::contexts",
                    analysed = analysed.len(),
                    queued = wanted.queue.len() + 1,
                    "cfg context planning stopped before its fixed point"
                );
                wanted.queue.push_front(pending);
                break;
            }
            self.reporter.say(format!(
                "Analysing cfg context {} ({} queued): {context}",
                analysed.len() + 1,
                wanted.queue.len()
            ));
            let watch =
                crate::process_memory::Watch::start(format!("cfg context {}", analysed.len() + 1));
            cfgs.enter(&mut self.db, &context);
            crate::process_memory::log("cfg context entered");
            if pending.is_merged() && declares_a_name_twice(&self.db) {
                tracing::debug!(
                    target: "cordial::semantic::contexts",
                    %context,
                    members = pending.members().len(),
                    "merged cfg context declares a name twice; splitting it"
                );
                for half in pending.halves().into_iter().rev() {
                    wanted.queue.push_front(half);
                }
                continue;
            }
            let index = u32::try_from(analysed.len()).unwrap_or(u32::MAX);
            analysed.push(context.clone());
            let pairs = cfgs.pairs(&context);
            let files = Files { paths, nodes };
            let finished = self.harvest(&context, index, &pairs, &budget, &files, graph)?;
            crate::process_memory::log("cfg context harvested");
            if !finished {
                tracing::warn!(
                    target: "cordial::semantic::contexts",
                    %context,
                    "cfg context stopped by the memory budget; its facts are partial"
                );
                return Ok(false);
            }
            if automatic {
                wanted.add(asked(&self.db, &context, &mut covered));
            }
            drop(watch);
            tracing::debug!(
                target: "cordial::semantic::contexts",
                %context,
                analysed = analysed.len(),
                queued = wanted.queue.len(),
                "cfg context analysed"
            );
            crate::process_memory::log("cfg context analysed");
        }
        Ok(wanted.queue.is_empty())
    }

    /// Copy the active context's facts into `graph`.
    ///
    /// Every function that resolves records that it is active in this context.
    /// Full facts and call edges are kept only where this context's cfg change
    /// can matter: a function inactive in the ordinary build, one inside or
    /// holding a gate the context flips, or one that mentions a name such a gate
    /// defines. Returns `false` when the memory budget ended the pass early.
    #[instrument(level = "debug", skip_all, fields(context = %context), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn harvest(
        &self,
        context: &SemanticContextKey,
        index: u32,
        pairs: &[(CfgOptions, CfgOptions)],
        budget: &Budget,
        files: &Files<'_>,
        graph: &mut CrateIr,
    ) -> CordialResult<bool> {
        attach_db(&self.db, || {
            let sema = Semantics::new(&self.db);
            let mut definitions: HashMap<SemanticItemId, NodeId> = HashMap::new();
            let mut calls: Vec<(NodeId, SemanticItemId)> = Vec::new();
            let mut finished = true;
            for path in files.paths {
                if budget.exceeded() {
                    finished = false;
                    break;
                }
                let file = self.file_id(path)?;
                let tree = sema.parse(sema.attach_first_edition(file));
                let flips = Flips::collect(&tree, pairs);
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
                    definitions.insert(types::identity(&self.db, definition.into()), node);
                    push_activity(graph, node, index)?;
                    let new = crate::IrView::semantic_function(graph, node)?.is_none();
                    if !(new || flips.touches(range) || flips.mentions(function.syntax())) {
                        continue;
                    }
                    let facts = function_evidence(&self.db, FunctionKey::new(&self.db, definition))
                        .as_ref()
                        .ok_or_else(|| {
                            crate::CordialError::workspace_source_facts_missing(
                                "semantic function source",
                            )
                        })?
                        .clone();
                    for call in facts.calls() {
                        if let crate::SemanticCallTarget::Function(target) = call.target() {
                            calls.push((node, *target));
                        }
                    }
                    push_facts(
                        graph,
                        node,
                        SemanticContextFunction::new(context.clone(), facts),
                    )?;
                }
            }
            let mut edges: BTreeSet<(NodeId, NodeId)> = BTreeSet::new();
            for (caller, target) in calls {
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
                }
            }
            for (caller, callee) in edges {
                graph.insert_context_call(caller, callee, context.clone())?;
            }
            Ok(finished)
        })
    }
}

/// The files of one projection and the graph node of each function in them.
struct Files<'a> {
    paths: &'a [PathBuf],
    nodes: &'a FunctionNodes,
}

/// How much more resident memory the context passes may use than the ordinary
/// pass left behind. Rust-analyzer keeps every body it infers, so memory grows
/// with the code a context newly activates; this stops planning cleanly instead
/// of letting it take the machine.
#[derive(Debug, Clone, Copy)]
struct Budget {
    ceiling_kib: Option<u64>,
}

/// Growth allowed when the options do not say.
const DEFAULT_BUDGET_MIB: u64 = 1024;

impl Budget {
    #[instrument(level = "debug", skip(options))]
    fn start(options: &crate::SemanticCargoOptions) -> Self {
        let allowed_kib = options
            .context_memory_budget_mib()
            .unwrap_or(DEFAULT_BUDGET_MIB)
            .saturating_mul(1024);
        // A budget of zero allows nothing, so planning stops before the first
        // context whatever the process happens to be using.
        let ceiling_kib = crate::memory_sample().map(|memory| {
            if allowed_kib == 0 {
                0
            } else {
                memory.resident_kib().saturating_add(allowed_kib)
            }
        });
        Self { ceiling_kib }
    }

    /// Whether resident memory is past the ceiling. Without a reading, never.
    #[instrument(level = "trace", skip(self))]
    fn exceeded(&self) -> bool {
        match (self.ceiling_kib, crate::memory_sample()) {
            (Some(ceiling), Some(memory)) => memory.resident_kib() > ceiling,
            _ => false,
        }
    }
}

/// Record that `node` is active in the context at `index`.
#[instrument(level = "trace", skip(graph, node), err(level = "warn"))]
fn push_activity(graph: &mut CrateIr, node: NodeId, index: u32) -> CordialResult<()> {
    let mut active: Vec<u32> = crate::IrView::node(graph, node)
        .and_then(|node| node.attr(crate::ATTR_SEMANTIC_ACTIVITY).cloned())
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or_default();
    active.push(index);
    graph.set_attr(
        node,
        crate::ATTR_SEMANTIC_ACTIVITY,
        serde_json::to_value(active)?,
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
