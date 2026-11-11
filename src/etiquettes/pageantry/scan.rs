//! Adapters over loader-owned pageantry evidence.
use super::types::PageantrySiteRecord;
use crate::{CordialResult, PageantryThresholds};
use std::path::Path;

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
