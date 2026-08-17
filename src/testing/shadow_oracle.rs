//! Inventory-oracle shadow pair reports (parity tests only).

use tracing::instrument;

use crate::error::CordialResult;
use crate::ir::{resolve_crate_root, shadow_dep_rustdoc_path};
use crate::rustdoc::{
    RustdocInventory, collect_trait_impl_map_from_inventory, collect_type_methods_from_inventory,
    parse_rustdoc_json,
};
use crate::session::SessionView;
use crate::shadow::{
    ShadowBuildMaps, ShadowReport, build_shadow_report_from_inventories_with_maps,
};

/// Build one upstream ↔ shadow mirror report by re-parsing rustdoc JSON (oracle).
#[instrument(skip(session), fields(upstream, shadow))]
pub fn build_shadow_pair_report(
    session: &dyn SessionView,
    upstream: &str,
    shadow: &str,
) -> CordialResult<ShadowReport> {
    let target = load_upstream_inventory(session, upstream, shadow)?;
    let shadow_inv = load_crate_inventory(session, shadow)?;
    Ok(build_shadow_pair_report_from_inventories(
        &target,
        &shadow_inv,
    ))
}

pub fn build_shadow_pair_report_from_inventories(
    target: &RustdocInventory,
    shadow: &RustdocInventory,
) -> ShadowReport {
    let target_methods = collect_type_methods_from_inventory(target);
    let shadow_methods = collect_type_methods_from_inventory(shadow);
    let target_trait_impls = collect_trait_impl_map_from_inventory(target);
    let shadow_trait_impls = collect_trait_impl_map_from_inventory(shadow);
    let maps = ShadowBuildMaps {
        target_methods: &target_methods,
        shadow_methods: &shadow_methods,
        target_trait_impls: &target_trait_impls,
        shadow_trait_impls: &shadow_trait_impls,
    };
    build_shadow_report_from_inventories_with_maps(target, shadow, &maps)
}

fn load_crate_inventory(
    session: &dyn SessionView,
    crate_name: &str,
) -> CordialResult<RustdocInventory> {
    let crate_root = resolve_crate_root(session.project_root(), crate_name);
    let json_path = crate::rustdoc_loader::resolve_rustdoc_json(
        &crate_root,
        crate_name,
        Some(session.store_root()),
    )?;
    parse_rustdoc_json(&json_path, crate_name)
}

fn load_upstream_inventory(
    session: &dyn SessionView,
    upstream: &str,
    shadow: &str,
) -> CordialResult<RustdocInventory> {
    if let Some(inventory) = load_shadow_dep_upstream_inventory(session, shadow, upstream)? {
        return Ok(inventory);
    }
    load_crate_inventory(session, upstream)
}

fn load_shadow_dep_upstream_inventory(
    session: &dyn SessionView,
    shadow: &str,
    upstream: &str,
) -> CordialResult<Option<RustdocInventory>> {
    let Some(path) = shadow_dep_rustdoc_path(session.store_root(), shadow, upstream) else {
        return Ok(None);
    };
    Ok(Some(parse_rustdoc_json(&path, upstream)?))
}
