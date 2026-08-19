//! Combined internal error type graph and compliance scan.

use std::path::Path;

use crate::error::CordialResult;

use super::architecture::scan_crate_error_architecture;
use super::compliance::scan_crate_internal_error_compliance;
use super::type_graph::scan_crate_internal_error_type_graph;
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
    let type_graph = scan_crate_internal_error_type_graph(crate_root, crate_name)?;
    let mut compliance = scan_crate_internal_error_compliance(crate_root, crate_name)?;
    compliance
        .findings
        .extend(scan_crate_error_architecture(crate_root, crate_name)?);
    compliance.findings.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.rule_id.to_string().cmp(&b.rule_id.to_string()))
    });
    Ok(InternalErrorChainScanReport {
        crate_name: crate_name.to_string(),
        type_graph,
        compliance,
    })
}
