//! Combined internal error type graph and compliance scan.

use std::path::Path;

use crate::error::CordialResult;

use super::types::InternalErrorChainScanReport;

pub use super::compliance::scan_compliance_rust_source;
pub use super::type_graph::scan_error_rust_source;
use tracing::instrument;

/// Scan type graph and compliance for one crate.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_internal_error_chain(
    crate_root: &Path,
    crate_name: &str,
) -> CordialResult<InternalErrorChainScanReport> {
    Ok(
        crate::enricher::scan_crate_error_ir_with_absolute_paths(crate_root, crate_name)?
            .internal_report(crate_name),
    )
}
