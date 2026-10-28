//! Inventory-oracle wrapper coverage (parity tests only).

use std::collections::HashMap;
use std::path::Path;

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

/// Build wrapper coverage from hub trenchcoat pairs by re-parsing rustdoc JSON (oracle).
#[instrument(level = "info", skip(filter), err(level = "warn"))]
pub fn load_workspace_wrapper_coverage(
    project_root: &Path,
    store_root: &Path,
    filter: &dyn RunFilter,
) -> CordialResult<WrapperCoverageMap> {
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
        .map(crate::rustdoc::TrenchcoatPair::into_foreign_wrapper_pair)
        .collect();
    let complete = collect_elicit_complete_from_inventory(&inventory);
    let wrapper_prereqs: HashMap<_, _> = collect_trait_prereqs_for_inventory(&inventory);
    Ok(build_wrapper_coverage_map(
        &pairs,
        &complete,
        &wrapper_prereqs,
    ))
}
