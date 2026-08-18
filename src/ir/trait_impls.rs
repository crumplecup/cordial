//! Collect trait-impl type paths from graph IR.

use std::collections::HashSet;

use crate::ir::{BasicQuery, EdgeKind, IrView, NodeKind, NodeView};

use tracing::instrument;
static ALL_ITEMS: BasicQuery = BasicQuery {
    node_kinds: Vec::new(),
    edge_kinds: Vec::new(),
    attr_key: None,
    attr_value: None,
};

/// Type paths in `ir` that have `impl {trait_short} for T`.
#[instrument(level = "debug", skip(ir))]
pub fn collect_trait_impl_type_paths_from_ir(
    ir: &dyn IrView,
    trait_short: &str,
) -> HashSet<String> {
    let mut paths = HashSet::new();
    for node in ir.nodes_matching(&ALL_ITEMS) {
        if !matches!(node.kind(), NodeKind::Item(_)) {
            continue;
        }
        let Some(type_path) = node.attr("qualified_path").and_then(|value| value.as_str()) else {
            continue;
        };
        if type_has_trait_short(ir, node.id(), trait_short) {
            paths.insert(type_path.to_string());
        }
    }
    paths
}

fn type_has_trait_short(ir: &dyn IrView, type_node: crate::ir::NodeId, trait_short: &str) -> bool {
    ir.children(type_node, EdgeKind::Implements)
        .into_iter()
        .filter_map(|trait_node| ir.node(trait_node))
        .any(|trait_node| {
            trait_node
                .attr("trait_short")
                .and_then(|value| value.as_str())
                .is_some_and(|short| short == trait_short)
                || trait_node
                    .attr("qualified_path")
                    .and_then(|value| value.as_str())
                    .is_some_and(|path| path.rsplit("::").next() == Some(trait_short))
        })
}
