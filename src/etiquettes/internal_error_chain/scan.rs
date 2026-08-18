//! Combined internal error type graph and compliance scan.

use std::path::Path;

use crate::error::CordialResult;

use super::compliance::scan_crate_internal_error_compliance;
use super::type_graph::scan_crate_internal_error_type_graph;
use super::types::InternalErrorChainScanReport;

use tracing::instrument;
pub use super::compliance::scan_compliance_rust_source;
pub use super::type_graph::scan_error_rust_source;

/// Scan type graph and compliance for one crate.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_internal_error_chain(
    crate_root: &Path,
    crate_name: &str,
) -> CordialResult<InternalErrorChainScanReport> {
    let type_graph = scan_crate_internal_error_type_graph(crate_root, crate_name)?;
    let compliance = scan_crate_internal_error_compliance(crate_root, crate_name)?;
    Ok(InternalErrorChainScanReport {
        crate_name: crate_name.to_string(),
        type_graph,
        compliance,
    })
}
