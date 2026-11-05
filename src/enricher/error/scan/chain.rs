//! Remaining compatibility collection for internal architecture facts.

use super::ErrorIrScanReport;
use crate::error::CordialResult;
use crate::loader::SourceLoadView;
use std::path::Path;
use tracing::instrument;

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
            crate::ir::SourceErrorArchitecture::collect(source, crate_root, crate_name)?.assess(
                kind_fanout_max_call_sites,
                field_strategy,
                false,
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
