use crate::etiquettes::error_sites::PartitionedErrorSiteRow;
use crate::etiquettes::error_sites::{ErrorOriginClass, infer_foreign_error_type};

use super::types::{ErrorSitePartitionReport, ForeignErrorTypeRecord, ForeignErrorTypeReport};

/// Infer foreign error types from partitioned Phase 2 rows.
pub fn build_foreign_error_type_report(
    report: &ErrorSitePartitionReport,
) -> ForeignErrorTypeReport {
    let findings = report
        .findings
        .iter()
        .filter_map(infer_foreign_error_type_finding)
        .collect();

    ForeignErrorTypeReport {
        crate_name: report.crate_name.clone(),
        findings,
    }
}

fn infer_foreign_error_type_finding(
    finding: &PartitionedErrorSiteRow,
) -> Option<ForeignErrorTypeRecord> {
    if finding.origin_class == ErrorOriginClass::Internal {
        return None;
    }

    let (foreign_error_type, rule_id, confidence) =
        infer_foreign_error_type(&finding.source_snippet)?;

    Some(ForeignErrorTypeRecord {
        crate_name: finding.crate_name.clone(),
        foreign_error_type,
        rule_id,
        confidence,
        chain_break: finding.kind.map_err_is_chain_break(false),
        kind: finding.kind,
        context: finding.context.clone(),
        file: finding.file.clone(),
        line: finding.line,
        source_snippet: finding.source_snippet.clone(),
        site_snippet: finding.site_snippet.clone(),
    })
}

/// Build a partition report from partitioned rows.
pub fn build_error_site_partition_report(
    crate_name: &str,
    findings: Vec<PartitionedErrorSiteRow>,
) -> ErrorSitePartitionReport {
    ErrorSitePartitionReport {
        crate_name: crate_name.to_string(),
        findings,
    }
}
