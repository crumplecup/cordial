//! Adapters over loader-owned pageantry evidence.
use super::types::PageantrySiteRecord;
use crate::{
    CordialResult, PageantryThresholds, ir::SourcePolicyInventory, loader::SourceLoadView,
};
use std::path::Path;

/// Scan a crate through the shared loader inventory.
#[tracing::instrument(level = "debug", skip(thresholds), err(level = "warn"))]
pub fn scan_crate_pageantry(
    root: &Path,
    thresholds: &PageantryThresholds,
) -> CordialResult<Vec<PageantrySiteRecord>> {
    let source = SourceLoadView::for_standalone_scan().with_workspace_root(root.to_path_buf());
    Ok(SourcePolicyInventory::collect(&source, root)?
        .pageantry()
        .iter()
        .filter_map(|site| site.selected(thresholds))
        .collect())
}

/// Scan supplied source through the loader-owned native database.
#[tracing::instrument(level = "debug", skip(source, thresholds, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    tree_root: &Path,
    root: &Path,
    thresholds: &PageantryThresholds,
) -> CordialResult<Vec<PageantrySiteRecord>> {
    Ok(
        crate::ir::scan_pageantry_source(source, file, tree_root, root)?
            .iter()
            .filter_map(|site| site.selected(thresholds))
            .collect(),
    )
}
