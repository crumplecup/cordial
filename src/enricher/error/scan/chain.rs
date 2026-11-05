//! Remaining compatibility collection for internal architecture facts.

use super::ErrorIrScanReport;
use crate::error::CordialResult;
use crate::loader::SourceLoadView;
use std::path::Path;
use tracing::instrument;

#[instrument(level = "debug", skip(source, field_strategy))]
pub(in crate::enricher::error) fn scan_crate_error_ir_with_source(
    crate_root: &Path,
    crate_name: &str,
    source: &SourceLoadView,
    kind_fanout_max_call_sites: usize,
    field_strategy: crate::config::ErrorFieldStrategy,
) -> CordialResult<ErrorIrScanReport> {
    scan_crate_error_ir_impl(
        crate_root,
        crate_name,
        source,
        kind_fanout_max_call_sites,
        field_strategy,
        true,
    )
}

#[instrument(level = "debug", skip(source, field_strategy))]
pub(super) fn scan_crate_error_ir_impl(
    crate_root: &Path,
    crate_name: &str,
    source: &SourceLoadView,
    kind_fanout_max_call_sites: usize,
    field_strategy: crate::config::ErrorFieldStrategy,
    relative_paths: bool,
) -> CordialResult<ErrorIrScanReport> {
    let mut report = ErrorIrScanReport::default();

    {
        report.compliance.extend(
            crate::etiquettes::internal_error_chain::scan_crate_error_architecture_with_source(
                crate_root,
                crate_name,
                source,
                kind_fanout_max_call_sites,
                field_strategy,
            )?,
        );
        super::compliance_sort::sort_compliance(&mut report.compliance);
        if relative_paths {
            for finding in &mut report.compliance {
                finding.strip_file_prefix(crate_root);
            }
        }
    }

    Ok(report)
}
