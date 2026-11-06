//! Error-site entry points using loader-issued source snapshots.

use std::path::Path;

use crate::error::CordialResult;
use crate::etiquettes::error_sites::ErrorSiteRecord;
use crate::loader::SourceLoadView;

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
    SourceLoadView::collect_native_fixture(file, source, |syntax| {
        Ok(
            crate::ir::SourceErrorSites::native_file(syntax, tree_root, crate_root)?
                .sites()
                .clone(),
        )
    })
}
