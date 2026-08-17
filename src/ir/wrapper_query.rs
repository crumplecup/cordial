//! Wrapper coverage queries from hub crate graph IR.

use std::collections::{HashMap, HashSet};

use crate::ir::{
    BasicQuery, CrateIr, IrView, NodeKind, WorkspaceIr,
    attrs::{
        ATTR_ELICIT_COMPLETE, ATTR_ELICIT_COMPLETE_FACTORY, ATTR_QUALIFIED_PATH,
        ATTR_TRAIT_PREREQS, ATTR_WRAPS_FOREIGN,
    },
};
use crate::rustdoc::{
    ElicitCompleteSet, TraitPrereqs, WrapperCoverageMap, build_wrapper_coverage_map,
};

/// Build wrapper coverage for foreign types from hub crate IR attrs and edges.
pub fn build_wrapper_coverage_from_hub_ir(
    workspace: &WorkspaceIr,
    hub_name: &str,
) -> WrapperCoverageMap {
    let Some(ir) = workspace.crate_ir(hub_name) else {
        return WrapperCoverageMap::new();
    };
    let pairs = collect_trenchcoat_pairs_from_ir(ir);
    let complete = elicit_complete_set_from_ir(ir);
    let prereqs = trait_prereqs_map_from_ir(ir);
    build_wrapper_coverage_map(&pairs, &complete, &prereqs)
}

/// `(foreign_path, wrapper_path)` pairs from materialized `wraps_foreign` attrs.
pub fn collect_trenchcoat_pairs_from_ir(ir: &CrateIr) -> Vec<(String, String)> {
    static ALL_NODES: BasicQuery = BasicQuery {
        node_kinds: Vec::new(),
        edge_kinds: Vec::new(),
        attr_key: None,
        attr_value: None,
    };

    let mut pairs = Vec::new();
    let mut seen = HashSet::new();
    for node in ir.nodes_matching(&ALL_NODES) {
        if !matches!(node.kind(), NodeKind::Item(_)) {
            continue;
        }
        let Some(wrapper_path) = node.attr(ATTR_QUALIFIED_PATH).and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(foreign_path) = node.attr(ATTR_WRAPS_FOREIGN).and_then(|v| v.as_str()) else {
            continue;
        };
        let key = (foreign_path.to_string(), wrapper_path.to_string());
        if seen.insert(key.clone()) {
            pairs.push(key);
        }
    }
    pairs.sort();
    pairs
}

fn elicit_complete_set_from_ir(ir: &CrateIr) -> ElicitCompleteSet {
    static ALL_NODES: BasicQuery = BasicQuery {
        node_kinds: Vec::new(),
        edge_kinds: Vec::new(),
        attr_key: None,
        attr_value: None,
    };

    let mut concrete = HashSet::new();
    let mut factory = HashSet::new();
    for node in ir.nodes_matching(&ALL_NODES) {
        let Some(path) = node.attr(ATTR_QUALIFIED_PATH).and_then(|v| v.as_str()) else {
            continue;
        };
        if !node
            .attr(ATTR_ELICIT_COMPLETE)
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        if node
            .attr(ATTR_ELICIT_COMPLETE_FACTORY)
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            factory.insert(path.to_string());
        } else {
            concrete.insert(path.to_string());
        }
    }
    ElicitCompleteSet { concrete, factory }
}

fn trait_prereqs_map_from_ir(ir: &CrateIr) -> HashMap<String, TraitPrereqs> {
    static ALL_NODES: BasicQuery = BasicQuery {
        node_kinds: Vec::new(),
        edge_kinds: Vec::new(),
        attr_key: None,
        attr_value: None,
    };

    ir.nodes_matching(&ALL_NODES)
        .into_iter()
        .filter_map(|node| {
            let path = node.attr(ATTR_QUALIFIED_PATH).and_then(|v| v.as_str())?;
            let prereqs = node
                .attr(ATTR_TRAIT_PREREQS)
                .and_then(|value| serde_json::from_value(value.clone()).ok())?;
            Some((path.to_string(), prereqs))
        })
        .collect()
}

/// Compare IR-built map with inventory oracle for parity tests.
pub fn wrapper_maps_equivalent(left: &WrapperCoverageMap, right: &WrapperCoverageMap) -> bool {
    if left.len() != right.len() {
        return false;
    }
    for (foreign, wrappers) in left {
        let Some(other) = right.get(foreign) else {
            return false;
        };
        if wrappers != other {
            return false;
        }
    }
    true
}
