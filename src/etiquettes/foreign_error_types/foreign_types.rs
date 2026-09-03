use crate::error::CordialResult;
use crate::etiquettes::error_sites::PartitionedErrorSiteRow;
use crate::etiquettes::error_sites::{ErrorOriginClass, infer_foreign_error_type};

use super::types::{ErrorSitePartitionReport, ForeignErrorTypeRecord, ForeignErrorTypeReport};

use tracing::instrument;
/// Infer foreign error types from partitioned Phase 2 rows.
#[instrument(level = "debug", skip(report), err(level = "warn"))]
pub fn build_foreign_error_type_report(
    report: &ErrorSitePartitionReport,
) -> CordialResult<ForeignErrorTypeReport> {
    let findings = report
        .findings()
        .iter()
        .filter(|finding| finding.origin_class() != ErrorOriginClass::Internal)
        .filter_map(|finding| infer_foreign_error_type_finding(finding).transpose())
        .collect::<CordialResult<Vec<_>>>()?;

    Ok(ForeignErrorTypeReport::new(
        report.crate_name().clone(),
        findings,
    ))
}

#[instrument(level = "debug", skip(finding), err(level = "warn"))]
fn infer_foreign_error_type_finding(
    finding: &PartitionedErrorSiteRow,
) -> CordialResult<Option<ForeignErrorTypeRecord>> {
    let Some((foreign_error_type, rule_id, confidence)) =
        infer_foreign_error_type(finding.source_snippet())
    else {
        return Ok(None);
    };

    Ok(Some(
        ForeignErrorTypeRecord::builder()
            .crate_name(finding.crate_name().clone())
            .foreign_error_type(foreign_error_type)
            .rule_id(rule_id)
            .confidence(confidence)
            .chain_break(finding.kind().map_err_is_chain_break(false))
            .kind(finding.kind())
            .context(finding.context().clone())
            .file(finding.file().clone())
            .line(finding.line())
            .source_snippet(finding.source_snippet().clone())
            .site_snippet(finding.site_snippet().clone())
            .build()?,
    ))
}

/// Build a partition report from partitioned rows.
#[instrument(level = "debug", skip(findings))]
pub fn build_error_site_partition_report(
    crate_name: &str,
    findings: Vec<PartitionedErrorSiteRow>,
) -> ErrorSitePartitionReport {
    ErrorSitePartitionReport::new(crate_name.to_string(), findings)
}
