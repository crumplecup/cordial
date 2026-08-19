//! Shadow mirror compare from workspace graph IR (attrs on type/trait nodes).

use std::collections::{BTreeSet, HashMap, HashSet};

use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::ir::{
    ATTR_ELICIT_COMPLETE, ATTR_ELICIT_COMPLETE_FACTORY, ATTR_ITEM_NAME, ATTR_PUBLIC_METHODS,
    ATTR_QUALIFIED_PATH, ATTR_RUSTDOC_KIND, ATTR_TRAIT_IMPLS, ATTR_TRAIT_PREREQS, BasicQuery,
    EdgeKind, IrView, NodeKind, WorkspaceIr,
};
use crate::rustdoc::{ElicitCompleteSet, InventoryItemKind, RustdocItem, TraitPrereqs};

use super::matching::{counts_toward_shadow_kind, normalize_name};
use super::report::build_shadow_report;
use super::types::{ShadowBuildMaps, ShadowReport};

/// One public inventory row materialized from graph IR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowIrItem {
    pub path: String,
    pub name: String,
    pub kind: InventoryItemKind,
    pub is_public: bool,
    pub public_methods: BTreeSet<String>,
    pub trait_impls: BTreeSet<String>,
    pub trait_prereqs: Option<TraitPrereqs>,
    pub elicit_complete: bool,
    pub elicit_complete_factory: bool,
}

impl ShadowIrItem {
    #[instrument(level = "trace", skip(self))]
    pub fn to_rustdoc_item(&self) -> RustdocItem {
        RustdocItem {
            path: self.path.clone(),
            name: self.name.clone(),
            kind: self.kind,
            is_public: self.is_public,
        }
    }
}

/// Collect shadow-relevant items from one crate's graph IR.
#[instrument(level = "debug", skip(workspace), err(level = "warn"))]
pub fn collect_shadow_items_from_workspace(
    workspace: &WorkspaceIr,
    crate_name: &str,
) -> CordialResult<Vec<ShadowIrItem>> {
    let ir = workspace
        .crate_ir(crate_name)
        .ok_or_else(|| missing_crate_ir(crate_name))?;
    Ok(collect_shadow_items_from_ir(ir))
}

#[instrument(level = "debug", skip(ir))]
pub fn collect_shadow_items_from_ir(ir: &crate::ir::CrateIr) -> Vec<ShadowIrItem> {
    static ALL_NODES: BasicQuery = BasicQuery {
        node_kinds: Vec::new(),
        edge_kinds: Vec::new(),
        attr_key: None,
        attr_value: None,
    };

    ir.nodes_matching(&ALL_NODES)
        .into_iter()
        .filter_map(|node| {
            if !matches!(node.kind(), NodeKind::Item(_)) {
                return None;
            }
            let path = node.attr(ATTR_QUALIFIED_PATH).and_then(|v| v.as_str())?;
            let kind = inventory_kind_from_attr(node.attr(ATTR_RUSTDOC_KIND)?.as_str()?)?;
            if !counts_toward_shadow_kind(kind) {
                return None;
            }
            let name = node
                .attr(ATTR_ITEM_NAME)
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| path.rsplit("::").next().unwrap_or(path))
                .to_string();
            let is_public = node
                .attr("is_public")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let public_methods = string_set_attr(&node, ATTR_PUBLIC_METHODS);
            let trait_impls = string_set_attr(&node, ATTR_TRAIT_IMPLS);
            let trait_prereqs = node
                .attr(ATTR_TRAIT_PREREQS)
                .and_then(|value| serde_json::from_value(value.clone()).ok());
            let elicit_complete = node
                .attr(ATTR_ELICIT_COMPLETE)
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let elicit_complete_factory = node
                .attr(ATTR_ELICIT_COMPLETE_FACTORY)
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            Some(ShadowIrItem {
                path: path.to_string(),
                name,
                kind,
                is_public,
                public_methods,
                trait_impls,
                trait_prereqs,
                elicit_complete,
                elicit_complete_factory,
            })
        })
        .collect()
}

/// Match upstream ↔ shadow items and record [`EdgeKind::Mirrors`] cross-crate edges.
#[instrument(level = "debug", skip(workspace), err(level = "warn"))]
pub fn materialize_cross_crate_shadow_mirrors(
    workspace: &mut WorkspaceIr,
    upstream: &str,
    shadow: &str,
) -> CordialResult<()> {
    let target_items = collect_shadow_items_from_workspace(workspace, upstream)?;
    let shadow_items = collect_shadow_items_from_workspace(workspace, shadow)?;

    let mut shadow_by_name: HashMap<&str, Vec<&ShadowIrItem>> = HashMap::new();
    for item in &shadow_items {
        shadow_by_name
            .entry(item.name.as_str())
            .or_default()
            .push(item);
    }

    let mut shadow_normalized: HashMap<String, Vec<&ShadowIrItem>> = HashMap::new();
    for item in &shadow_items {
        if !counts_toward_shadow_kind(item.kind) {
            continue;
        }
        shadow_normalized
            .entry(normalize_name(&item.name))
            .or_default()
            .push(item);
    }

    let mut matched_pairs: Vec<(String, String)> = Vec::new();
    for target_item in &target_items {
        let shadow_item = shadow_by_name
            .get(target_item.name.as_str())
            .and_then(|candidates| {
                candidates
                    .iter()
                    .find(|candidate| candidate.kind == target_item.kind)
                    .or_else(|| candidates.first())
                    .copied()
            })
            .or_else(|| find_drift_shadow_item(target_item, &shadow_normalized));

        if let Some(shadow_item) = shadow_item {
            matched_pairs.push((target_item.path.clone(), shadow_item.path.clone()));
        }
    }

    for (target_path, shadow_path) in matched_pairs {
        let Some(target_node) = workspace
            .crate_ir(upstream)
            .and_then(|ir| ir.node_by_path(&target_path))
        else {
            continue;
        };
        let Some(shadow_node) = workspace
            .crate_ir(shadow)
            .and_then(|ir| ir.node_by_path(&shadow_path))
        else {
            continue;
        };
        if !has_cross_crate_mirror(workspace, upstream, target_node, shadow, shadow_node) {
            workspace.insert_cross_crate_edge(
                upstream,
                target_node,
                shadow,
                shadow_node,
                EdgeKind::Mirrors,
            );
        }
    }

    Ok(())
}

/// Build one upstream ↔ shadow mirror report from workspace graph IR.
#[instrument(level = "debug", skip(workspace), err(level = "warn"))]
pub fn build_shadow_pair_report_from_workspace_ir(
    workspace: &WorkspaceIr,
    upstream: &str,
    shadow: &str,
) -> CordialResult<ShadowReport> {
    let target_items = collect_shadow_items_from_workspace(workspace, upstream)?;
    let shadow_items = collect_shadow_items_from_workspace(workspace, shadow)?;

    let target_methods = methods_map_from_items(&target_items);
    let shadow_methods = methods_map_from_items(&shadow_items);
    let target_trait_impls = trait_impl_map_from_items(upstream, &target_items);
    let shadow_trait_impls = trait_impl_map_from_items(shadow, &shadow_items);
    let maps = ShadowBuildMaps {
        target_methods: &target_methods,
        shadow_methods: &shadow_methods,
        target_trait_impls: &target_trait_impls,
        shadow_trait_impls: &shadow_trait_impls,
    };

    let target = rustdoc_inventory_from_items(upstream, &target_items);
    let shadow_inv = rustdoc_inventory_from_items(shadow, &shadow_items);
    let shadow_complete = elicit_complete_set_from_items(&shadow_items);
    let shadow_prereqs = prereqs_map_from_items(&shadow_items);

    Ok(build_shadow_report(
        &target,
        &shadow_inv,
        &shadow_complete,
        &shadow_prereqs,
        &maps,
    ))
}

#[instrument(level = "debug", skip(items))]
fn rustdoc_inventory_from_items(
    crate_name: &str,
    items: &[ShadowIrItem],
) -> crate::rustdoc::RustdocInventory {
    crate::rustdoc::RustdocInventory {
        crate_name: crate_name.to_string(),
        crate_version: String::new(),
        items: items.iter().map(|item| item.to_rustdoc_item()).collect(),
        krate: empty_krate(),
    }
}

#[instrument(level = "debug")]
fn empty_krate() -> rustdoc_types::Crate {
    rustdoc_types::Crate {
        root: rustdoc_types::Id(0),
        crate_version: None,
        includes_private: false,
        index: Default::default(),
        paths: Default::default(),
        external_crates: Default::default(),
        target: rustdoc_types::Target {
            triple: String::new(),
            target_features: Vec::new(),
        },
        format_version: rustdoc_types::FORMAT_VERSION,
    }
}

#[instrument(level = "debug", skip(items))]
fn methods_map_from_items(items: &[ShadowIrItem]) -> HashMap<String, BTreeSet<String>> {
    items
        .iter()
        .filter(|item| item.kind.is_type())
        .map(|item| (item.path.clone(), item.public_methods.clone()))
        .collect()
}

#[instrument(level = "debug", skip(items))]
fn trait_impl_map_from_items(
    crate_name: &str,
    items: &[ShadowIrItem],
) -> HashMap<String, BTreeSet<String>> {
    let mut map: HashMap<String, BTreeSet<String>> = HashMap::new();
    for item in items.iter().filter(|item| item.kind.is_type()) {
        let bare = item
            .path
            .rsplit("::")
            .next()
            .unwrap_or(item.name.as_str())
            .to_string();
        for trait_short in &item.trait_impls {
            let trait_path = items
                .iter()
                .find(|candidate| {
                    candidate.kind == InventoryItemKind::Trait
                        && (candidate.name == *trait_short || candidate.path.ends_with(trait_short))
                })
                .map(|candidate| candidate.path.clone())
                .unwrap_or_else(|| format!("{crate_name}::{trait_short}"));
            map.entry(trait_path).or_default().insert(bare.clone());
        }
    }
    map
}

#[instrument(level = "debug", skip(items))]
fn elicit_complete_set_from_items(items: &[ShadowIrItem]) -> ElicitCompleteSet {
    let mut concrete = HashSet::new();
    let mut factory = HashSet::new();
    for item in items
        .iter()
        .filter(|item| item.kind.is_type() && item.elicit_complete)
    {
        if item.elicit_complete_factory {
            factory.insert(item.path.clone());
        } else {
            concrete.insert(item.path.clone());
        }
    }
    ElicitCompleteSet { concrete, factory }
}

#[instrument(level = "debug", skip(items))]
fn prereqs_map_from_items(items: &[ShadowIrItem]) -> HashMap<String, TraitPrereqs> {
    items
        .iter()
        .filter_map(|item| {
            item.trait_prereqs
                .clone()
                .map(|prereqs| (item.path.clone(), prereqs))
        })
        .collect()
}

#[instrument(level = "debug")]
fn inventory_kind_from_attr(value: &str) -> Option<InventoryItemKind> {
    match value {
        "Struct" => Some(InventoryItemKind::Struct),
        "Enum" => Some(InventoryItemKind::Enum),
        "Trait" => Some(InventoryItemKind::Trait),
        "TypeAlias" => Some(InventoryItemKind::TypeAlias),
        "Function" => Some(InventoryItemKind::Function),
        "Other" => Some(InventoryItemKind::Other),
        _ => None,
    }
}

#[instrument(level = "debug", skip(node))]
fn string_set_attr(node: &crate::ir::NodeRef<'_>, key: &str) -> BTreeSet<String> {
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

#[instrument(level = "debug", skip(target_item, shadow_names))]
fn find_drift_shadow_item<'a>(
    target_item: &ShadowIrItem,
    shadow_names: &HashMap<String, Vec<&'a ShadowIrItem>>,
) -> Option<&'a ShadowIrItem> {
    let target_norm = normalize_name(&target_item.name);
    let mut best: Option<(&ShadowIrItem, f32)> = None;

    for (shadow_norm, candidates) in shadow_names {
        let dist = edit_distance(&target_norm, shadow_norm);
        let max_len = target_norm.len().max(shadow_norm.len());
        if max_len == 0 {
            continue;
        }
        let confidence = 1.0 - (dist as f32 / max_len as f32);
        if confidence < 0.75 {
            continue;
        }
        for shadow_item in candidates {
            if shadow_item.kind != target_item.kind {
                continue;
            }
            if best.is_none_or(|(_, score)| confidence > score) {
                best = Some((shadow_item, confidence));
            }
        }
    }

    best.map(|(item, _)| item)
}

#[instrument(level = "debug")]
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for (i, row) in dp.iter_mut().enumerate().take(m + 1) {
        row[0] = i;
    }
    for (j, cell) in dp[0].iter_mut().enumerate().take(n + 1) {
        *cell = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if a[i - 1] == b[j - 1] {
                dp[i - 1][j - 1]
            } else {
                1 + dp[i - 1][j].min(dp[i][j - 1]).min(dp[i - 1][j - 1])
            };
        }
    }
    dp[m][n]
}

#[instrument(level = "trace", skip(workspace, target_node, shadow_node), ret)]
fn has_cross_crate_mirror(
    workspace: &WorkspaceIr,
    upstream: &str,
    target_node: crate::ir::NodeId,
    shadow: &str,
    shadow_node: crate::ir::NodeId,
) -> bool {
    workspace
        .cross_crate_edges
        .iter()
        .any(|(from_crate, from, to_crate, to, weight)| {
            weight.kind == EdgeKind::Mirrors
                && from_crate == upstream
                && *from == target_node
                && to_crate == shadow
                && *to == shadow_node
        })
}

#[instrument(level = "debug")]
fn missing_crate_ir(crate_name: &str) -> CordialError {
    CordialError::invariant(format!(
        "crate `{crate_name}` IR not loaded in workspace for shadow compare"
    ))
}
