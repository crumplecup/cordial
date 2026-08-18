//! Inventory-oracle wrapper coverage (parity tests only).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use tracing::instrument;

use crate::error::CordialResult;
use crate::feature_probe::hub_crate_name;
use crate::plugin::{WorkspaceHub, discover_workspace_hub};
use crate::rustdoc::{
    WrapperCoverageMap, build_wrapper_coverage_map, collect_elicit_complete_from_inventory,
    collect_trait_prereqs_for_inventory, collect_trenchcoat_pairs, parse_rustdoc_json,
};
use crate::rustdoc_loader::resolve_rustdoc_json;
use crate::session::RunFilter;

/// Cache path for the serialized wrapper coverage map.
#[instrument(level = "trace")]
pub fn wrapper_coverage_cache_path(store_root: &Path) -> PathBuf {
    store_root.join("cache/wrapper-coverage.json")
}

/// Build wrapper coverage from hub trenchcoat pairs by re-parsing rustdoc JSON (oracle).
#[instrument(level = "info", skip(filter), err(level = "warn"))]
pub fn load_workspace_wrapper_coverage(
    project_root: &Path,
    store_root: &Path,
    filter: &dyn RunFilter,
) -> CordialResult<WrapperCoverageMap> {
    let cache_path = wrapper_coverage_cache_path(store_root);
    if cache_path.is_file() {
        let body = fs::read_to_string(&cache_path)?;
        if let Ok(map) = serde_json::from_str(&body) {
            return Ok(map);
        }
    }

    let hub = discover_workspace_hub(project_root, filter)?;
    if hub != WorkspaceHub::Elicitation {
        return Ok(WrapperCoverageMap::new());
    }

    let Some(hub_name) = hub_crate_name(hub) else {
        return Ok(WrapperCoverageMap::new());
    };

    let hub_root = project_root.join("crates").join(hub_name);
    let json_path = resolve_rustdoc_json(&hub_root, hub_name, Some(store_root))?;
    let inventory = parse_rustdoc_json(&json_path, hub_name)?;
    let pairs: Vec<(String, String)> = collect_trenchcoat_pairs(&inventory)
        .into_iter()
        .map(|pair| (pair.foreign_path, pair.wrapper_path))
        .collect();
    let complete = collect_elicit_complete_from_inventory(&inventory);
    let wrapper_prereqs: HashMap<_, _> = collect_trait_prereqs_for_inventory(&inventory);
    let map = build_wrapper_coverage_map(&pairs, &complete, &wrapper_prereqs);

    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&cache_path, serde_json::to_string_pretty(&map)?)?;

    Ok(map)
}
