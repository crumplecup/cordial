//! Read helpers for rustdoc facts materialized on the IR graph.

use crate::ir::{
    BasicQuery, CrateIr, EdgeKind, IrView, NodeId, NodeRef,
    attrs::{
        ATTR_ELICIT_COMPLETE, ATTR_PUBLIC_METHODS, ATTR_QUALIFIED_PATH, ATTR_RUSTDOC_KIND,
        ATTR_TRAIT_IMPLS, ATTR_TRAIT_PREREQS, ATTR_WRAPS_FOREIGN,
    },
};
use crate::rustdoc::{InventoryItemKind, TraitPrereqs};

use tracing::instrument;
fn node_for_path<'a>(ir: &'a dyn IrView, type_path: &str) -> Option<NodeRef<'a>> {
    ir.node_by_path(type_path).and_then(|id| ir.node(id))
}

fn string_list_attr(node: &NodeRef<'_>, key: &str) -> Vec<String> {
    node.attr(key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Public method names recorded on a type node.
#[instrument(level = "debug", skip(ir))]
pub fn type_public_methods(ir: &dyn IrView, type_path: &str) -> Vec<String> {
    node_for_path(ir, type_path)
        .map(|node| string_list_attr(&node, ATTR_PUBLIC_METHODS))
        .unwrap_or_default()
}

/// Trait short names implemented by a type (`Serialize`, `Deserialize`, …).
#[instrument(level = "debug", skip(ir))]
pub fn type_trait_impls(ir: &dyn IrView, type_path: &str) -> Vec<String> {
    node_for_path(ir, type_path)
        .map(|node| string_list_attr(&node, ATTR_TRAIT_IMPLS))
        .unwrap_or_default()
}

/// ElicitComplete supertrait prereqs materialized on a type node.
#[instrument(level = "debug", skip(ir))]
pub fn type_trait_prereqs(ir: &dyn IrView, type_path: &str) -> Option<TraitPrereqs> {
    let node = node_for_path(ir, type_path)?;
    let value = node.attr(ATTR_TRAIT_PREREQS)?;
    serde_json::from_value(value.clone()).ok()
}

/// Whether the type has a concrete or factory `ElicitComplete` impl in this crate.
#[instrument(level = "debug", skip(ir))]
pub fn type_elicit_complete(ir: &dyn IrView, type_path: &str) -> bool {
    node_for_path(ir, type_path)
        .and_then(|node| {
            node.attr(ATTR_ELICIT_COMPLETE)
                .and_then(|value| value.as_bool())
        })
        .unwrap_or(false)
}

/// Foreign type path wrapped via trenchcoat `From<T>` when present on the wrapper node.
#[instrument(level = "debug", skip(ir))]
pub fn type_wraps_foreign(ir: &dyn IrView, wrapper_path: &str) -> Option<String> {
    node_for_path(ir, wrapper_path).and_then(|node| {
        node.attr(ATTR_WRAPS_FOREIGN)
            .and_then(|value| value.as_str())
            .map(str::to_string)
    })
}

/// Paired node linked by [`EdgeKind::Mirrors`] (upstream → shadow or reverse).
#[instrument(level = "debug", skip(ir))]
pub fn mirror_target(ir: &dyn IrView, node: NodeId) -> Option<NodeId> {
    ir.children(node, EdgeKind::Mirrors)
        .into_iter()
        .next()
        .or_else(|| ir.parents(node, EdgeKind::Mirrors).into_iter().next())
}

/// All rustdoc-origin item nodes with a `qualified_path` attr.
#[instrument(level = "debug", skip(ir))]
pub fn rustdoc_item_nodes(ir: &dyn IrView) -> Vec<NodeRef<'_>> {
    static ALL_NODES: BasicQuery = BasicQuery {
        node_kinds: Vec::new(),
        edge_kinds: Vec::new(),
        attr_key: None,
        attr_value: None,
    };

    ir.nodes_matching(&ALL_NODES)
        .into_iter()
        .filter(|node| {
            node.attr(ATTR_QUALIFIED_PATH)
                .and_then(|v| v.as_str())
                .is_some()
        })
        .collect()
}

fn inventory_kind_from_attr(raw: &str) -> Option<InventoryItemKind> {
    match raw {
        "Struct" => Some(InventoryItemKind::Struct),
        "Enum" => Some(InventoryItemKind::Enum),
        "TypeAlias" => Some(InventoryItemKind::TypeAlias),
        "Trait" => Some(InventoryItemKind::Trait),
        "Function" => Some(InventoryItemKind::Function),
        "Other" => Some(InventoryItemKind::Other),
        _ => None,
    }
}

/// Count public type nodes materialized from rustdoc on one crate graph.
#[instrument(level = "debug")]
pub fn count_type_nodes(ir: &CrateIr) -> usize {
    rustdoc_item_nodes(ir)
        .into_iter()
        .filter(|node| {
            node.attr(ATTR_RUSTDOC_KIND)
                .and_then(|value| value.as_str())
                .and_then(inventory_kind_from_attr)
                .is_some_and(InventoryItemKind::is_type)
        })
        .count()
}
