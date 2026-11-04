//! Error-site entry points using loader-issued source snapshots.

use std::path::Path;

use crate::error::CordialResult;
use crate::etiquettes::error_sites::ErrorSiteRecord;
use crate::loader::{SharedParsedFile, SourceLoadView};

use tracing::instrument;
/// Scan one crate for error sites.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_error_sites(crate_root: &Path) -> CordialResult<Vec<ErrorSiteRecord>> {
    let source = SourceLoadView::for_standalone_scan();
    Ok(crate::ir::SourceErrorSites::collect(&source, crate_root)?.records())
}

/// Scan one Rust source file and return records.
#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<ErrorSiteRecord>> {
    let syntax = SourceLoadView::parse_fixture(file, source)?;
    scan_rust_syntax(&syntax, tree_root, crate_root)
}

/// Scan a pre-parsed file for error sites (via unified error IR visitor).
#[instrument(level = "debug", skip(file), err(level = "warn"))]
pub(crate) fn scan_rust_syntax(
    file: &SharedParsedFile,
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<ErrorSiteRecord>> {
    let src_root = crate_root.join("src");
    Ok(crate::etiquettes::scan_rust_file_syntax(
        file.syntax(),
        file.path(),
        tree_root,
        &src_root,
        crate_root,
        "",
        crate::etiquettes::ErrorIrScanLayers::SITES_ONLY,
    )?
    .sites()
    .clone())
}
