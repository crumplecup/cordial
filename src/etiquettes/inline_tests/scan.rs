//! Standalone adapters over loader-owned policy collection.
use crate::CordialResult;
use crate::etiquettes::inline_tests::InlineTestSiteRecord;
use crate::ir::SourcePolicyInventory;
pub use crate::ir::scan_inline_tests_source as scan_rust_source;
use crate::loader::SourceLoadView;
use std::path::Path;

/// Scan a crate through the shared loader inventory.
#[tracing::instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_inline_tests(crate_root: &Path) -> CordialResult<Vec<InlineTestSiteRecord>> {
    let source =
        SourceLoadView::for_standalone_scan().with_workspace_root(crate_root.to_path_buf());
    Ok(SourcePolicyInventory::collect(&source, crate_root)?
        .inline_tests()
        .clone())
}
