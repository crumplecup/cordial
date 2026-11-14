//! Populate source nodes and semantic evidence in the same ordinary IR graph.

use std::collections::HashMap;

use ra_ap_hir::Semantics;
use ra_ap_ide_db::RootDatabase;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasName},
};

use super::contexts::FunctionNodes;
use super::items;
use crate::ir::Resolved;
use crate::{
    CordialResult, CrateIr, EdgeKind, FunctionCallFlow, NodeId, ResolvedFunctionCall,
    SemanticItemId, SourceCallResolution,
};

/// Graph-local lookup tables, discarded once this projection is complete.
#[derive(Default)]
pub(super) struct Projection {
    definitions: HashMap<SemanticItemId, NodeId>,
    calls: Vec<(NodeId, SemanticItemId)>,
    /// Every function item's node, for the cfg-context passes that follow.
    functions: FunctionNodes,
    /// Each function's resolved calls, written once every file is in.
    flows: Vec<(NodeId, FunctionCallFlow)>,
    /// Whether any file withdraws trust from the crate's names.
    shadows_extern_crates: bool,
}

impl Projection {
    /// Insert one file's items into the graph and note what the passes after it
    /// need: each function's node, and the calls to link. The items are the
    /// file's memoised ones, built (or reused) by [`super::items::file_items`];
    /// this only inserts them, in the order they were built.
    #[tracing::instrument(level = "debug", skip(self, db, graph), err(level = "warn"))]
    #[hotpath::measure(impl_type = "Projection")]
    pub(super) fn populate(
        &mut self,
        db: &RootDatabase,
        file: ra_ap_vfs::FileId,
        path: &std::path::Path,
        src: &std::path::Path,
        graph: &mut CrateIr,
    ) -> CordialResult<NodeId> {
        let key = items::ItemsKey::new(db, file, path.to_path_buf(), src.to_path_buf());
        let owned;
        let file_items = match items::file_items(db, key) {
            Some(found) => found,
            // The query declines a file that did not parse; reading it directly
            // says why.
            None => {
                owned = items::collect_items(db, file, path, src)?;
                &owned
            }
        };
        let module = graph.insert_node(file_items.module().clone());
        graph.insert_edge(graph.root(), module, EdgeKind::Contains)?;
        let mut nodes: Vec<NodeId> = Vec::with_capacity(file_items.items().len());
        for record in file_items.items() {
            let node = graph.insert_node(record.weight().clone());
            let parent = record.parent().map_or(module, |index| nodes[index]);
            graph.insert_edge(parent, node, EdgeKind::Contains)?;
            if let Some(range) = record.function() {
                self.functions.insert(path, *range, node);
            }
            if let Some(definition) = record.definition() {
                self.definitions.insert(*definition, node);
                for target in record.calls() {
                    self.calls.push((node, *target));
                }
            }
            for island in record.islands() {
                let child = graph.insert_node(island.weight().clone());
                graph.insert_edge(node, child, EdgeKind::Contains)?;
                for target in island.calls() {
                    self.calls.push((child, *target));
                }
            }
            nodes.push(node);
        }
        self.shadows_extern_crates |= file_items.shadows_extern_crates();
        for (record, node) in file_items.items().iter().zip(&nodes) {
            let Some(flow) = record.flow() else {
                continue;
            };
            let calls = flow
                .calls()
                .iter()
                .map(|(range, resolved)| {
                    ResolvedFunctionCall::new(
                        range.clone(),
                        match resolved {
                            Resolved::Source {
                                target,
                                source_digest,
                            } => SourceCallResolution::Source {
                                function: nodes[*target],
                                source_digest: *source_digest,
                            },
                            Resolved::Unresolved(failure) => {
                                SourceCallResolution::Unresolved(*failure)
                            }
                        },
                    )
                })
                .collect();
            self.flows.push((
                *node,
                FunctionCallFlow::new(*flow.source_digest(), flow.trusted(), calls),
            ));
        }
        Ok(module)
    }

    /// Add the resolved `Calls` edges and hand back the function nodes.
    #[tracing::instrument(level = "trace", skip(self, graph, locator), err(level = "warn"))]
    pub(super) fn connect(
        self,
        graph: &mut CrateIr,
        locator: &super::external::Locator<'_>,
    ) -> CordialResult<FunctionNodes> {
        // One edge per caller and callee: the relationship. Each call's own
        // range and contract stay in the caller's semantic facts.
        let pairs: std::collections::BTreeSet<(NodeId, NodeId)> = self
            .calls
            .iter()
            .filter_map(|(caller, target)| {
                self.definitions
                    .get(target)
                    .map(|callee| (*caller, *callee))
            })
            .collect();
        for (caller, callee) in pairs {
            graph.insert_edge(caller, callee, EdgeKind::Calls)?;
        }
        for (node, flow) in self.flows {
            let flow = if self.shadows_extern_crates {
                // Names cannot be trusted anywhere in this crate: the calls stay
                // visible and authorize nothing.
                FunctionCallFlow::new(
                    flow.source_digest(),
                    false,
                    flow.calls()
                        .iter()
                        .map(|call| {
                            ResolvedFunctionCall::new(
                                call.range().clone(),
                                SourceCallResolution::Unresolved(
                                    crate::CallResolutionFailure::OpaqueScope,
                                ),
                            )
                        })
                        .collect(),
                )
            } else {
                flow
            };
            graph.set_attr(
                node,
                crate::ATTR_FUNCTION_CALL_FLOW,
                serde_json::to_value(flow)?,
            )?;
        }
        for (id, node) in &self.definitions {
            locator.record_trait_implementation(graph, *id, *node)?;
        }
        // A call to a function with no node here may land in another crate of
        // the workspace; record where, so that crate's graph can claim it.
        let mut external: std::collections::BTreeMap<
            NodeId,
            std::collections::BTreeSet<crate::FunctionLocation>,
        > = std::collections::BTreeMap::new();
        for (caller, target) in &self.calls {
            if !self.definitions.contains_key(target)
                && let Some(location) = locator.locate(*target)
            {
                external.entry(*caller).or_default().insert(location);
            }
        }
        for (caller, locations) in external {
            let calls: Vec<_> = locations
                .into_iter()
                .map(|location| crate::ExternalCall::new(location, None))
                .collect();
            graph.set_attr(
                caller,
                crate::ATTR_EXTERNAL_CALLS,
                serde_json::to_value(calls)?,
            )?;
        }
        Ok(self.functions)
    }
}

/// How many attribute macros may be stacked on one function before it is
/// reported as a gap. Every expansion that still carries a macro costs one.
const MAX_ATTRIBUTE_MACRO_DEPTH: usize = 8;

/// The definition of a function attribute macros replace, read from the function
/// they finally generate. With several macros stacked (`#[instrument]` over
/// `#[measure]`), each expansion still carries the next, so it is expanded again
/// until a definition appears. `None` when no macro applies or one cannot be
/// expanded (no proc-macro server, a macro error): that stays a gap, not a guess.
#[tracing::instrument(level = "trace", skip(sema, function))]
pub(super) fn expanded_definition(
    sema: &Semantics<'_, RootDatabase>,
    function: &ast::Fn,
) -> Option<ra_ap_hir::Function> {
    let name = function.name()?;
    let mut current = function.clone();
    for _ in 0..MAX_ATTRIBUTE_MACRO_DEPTH {
        let expansion = sema.expand_attr_macro(&ast::Item::Fn(current.clone()))?;
        let generated = expansion
            .value
            .value
            .descendants()
            .filter_map(ast::Fn::cast)
            .find(|candidate| {
                candidate
                    .name()
                    .is_some_and(|candidate| candidate.text() == name.text())
            })?;
        if let Some(definition) = sema.to_def(&generated) {
            return Some(definition);
        }
        current = generated;
    }
    None
}
