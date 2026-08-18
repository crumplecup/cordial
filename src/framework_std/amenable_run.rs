//! Amenable std registry coverage orchestration.

use std::path::Path;

use tracing::instrument;

use crate::error::CordialResult;
use crate::framework_std::amenable::{AmenableStdReport, build_amenable_std_report};
use crate::framework_std::inventory::load_merged_std_inventory;
use crate::framework_std::proof_harness::collect_proof_chain_subjects;
use crate::framework_std::registry::{
    RegistryDump, load_registry_dump, run_amenable_dump_registry,
};
use crate::framework_std::verifier_skip::load_verifier_skip_map;
use crate::session::SessionView;
use crate::store::{StoreLayout, SysrootCache};

pub const AMENABLE_IMPL_CRATE: &str = "amenable_std";
pub const AMENABLE_PATCH_SET: &str = "amenable";

/// Options for amenable std registry coverage assessment.
#[derive(Debug, Clone, Copy, Default)]
pub struct AmenableStdOptions {
    pub include_nightly: bool,
    /// Re-run `amenable dump-registry` even when a cached dump exists.
    pub refresh_registry: bool,
}

fn registry_dump_path(store: &StoreLayout) -> std::path::PathBuf {
    store.cache_dir().join("extracts/amenable-registry.json")
}

fn ensure_registry_dump(
    store: &StoreLayout,
    project_root: &Path,
    options: &AmenableStdOptions,
) -> CordialResult<RegistryDump> {
    let path = registry_dump_path(store);
    if !options.refresh_registry && path.is_file() {
        return load_registry_dump(&path);
    }
    run_amenable_dump_registry(project_root, &path)?;
    load_registry_dump(&path)
}

/// Load or refresh the cached amenable registry dump for assessors.
#[instrument(level = "debug", skip(options), err(level = "warn"))]
pub fn ensure_registry_dump_for_assessor(
    store: &StoreLayout,
    project_root: &Path,
    options: &AmenableStdOptions,
) -> CordialResult<RegistryDump> {
    ensure_registry_dump(store, project_root, options)
}

/// Assess amenable std registry coverage using sysroot inventory and registry dump.
#[instrument(level = "debug", skip(session, options), err(level = "warn"))]
pub fn assess_amenable_std_coverage(
    session: &dyn SessionView,
    store: &StoreLayout,
    sysroot: &SysrootCache,
    project_root: &Path,
    options: &AmenableStdOptions,
) -> CordialResult<AmenableStdReport> {
    let _ = session;
    let merged_items = load_merged_std_inventory(sysroot)?;
    let registry = ensure_registry_dump(store, project_root, options)?;
    let skip_map = load_verifier_skip_map(store, AMENABLE_PATCH_SET);
    let proof_chain_subjects = collect_proof_chain_subjects(project_root)?;
    Ok(build_amenable_std_report(
        "std",
        &merged_items,
        AMENABLE_IMPL_CRATE,
        &registry,
        &skip_map,
        &proof_chain_subjects,
        options.include_nightly,
    ))
}
