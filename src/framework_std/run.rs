//! Homecoming std (`Code`) coverage orchestration.

use std::path::Path;

use tracing::instrument;

use crate::error::CordialResult;
use crate::framework_std::inventory::load_merged_std_inventory;
use crate::framework_std::match_impl::collect_trait_impl_paths_from_json;
use crate::framework_std::skip::load_framework_skip_map;
use crate::framework_std::{FrameworkTraitReport, build_framework_trait_report};
use crate::session::SessionView;
use crate::store::{StoreLayout, SysrootCache};

pub const HOMECOMING_IMPL_CRATE: &str = "homecoming_core";
pub const HOMECOMING_TRAIT: &str = "Code";
pub const HOMECOMING_PATCH_SET: &str = "homecoming";

/// Options for framework std coverage assessment.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameworkStdOptions {
    pub include_nightly: bool,
}

/// Assess homecoming std `Code` coverage using cached rustdoc inventories.
#[instrument(
    level = "debug",
    skip(session, store, sysroot, options),
    err(level = "warn")
)]
pub fn assess_homecoming_std_coverage(
    session: &dyn SessionView,
    store: &StoreLayout,
    sysroot: &SysrootCache,
    project_root: &Path,
    options: &FrameworkStdOptions,
) -> CordialResult<FrameworkTraitReport> {
    let _ = session;
    let _ = project_root;
    let merged_items = load_merged_std_inventory(sysroot)?;
    let impl_json = store.rustdoc_cache_path(HOMECOMING_IMPL_CRATE);
    let impl_paths =
        collect_trait_impl_paths_from_json(&impl_json, HOMECOMING_IMPL_CRATE, HOMECOMING_TRAIT)?;
    let skip_map = load_framework_skip_map(store, HOMECOMING_PATCH_SET);
    Ok(build_framework_trait_report(
        "std",
        &merged_items,
        HOMECOMING_TRAIT,
        HOMECOMING_IMPL_CRATE,
        &impl_paths,
        &skip_map,
        options.include_nightly,
    ))
}
