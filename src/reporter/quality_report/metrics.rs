//! Open-finding counts for the hand-composed "Error handling" area --
//! the one area that genuinely merges several etiquettes (`panics`,
//! `foreign_error_attenuation`, `internal_error_chain`, two
//! `antipatterns` rule ids) rather than being any single etiquette's own
//! [`crate::etiquette::QualityReportArea`] contribution. Every other
//! area's metrics live with their owning etiquette module.

use std::collections::HashSet;

use crate::etiquette::{finding_field, open_findings};
use crate::objects::Finding;

use tracing::instrument;
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct ErrorHandlingMetrics {
    pub(super) chain_breaks: usize,
    pub(super) pending_infrastructure: usize,
    pub(super) neutral: usize,
    pub(super) compliance: usize,
    pub(super) compliance_unique: usize,
    pub(super) migration_backlog: usize,
}

#[instrument(level = "debug", skip(findings))]
pub(super) fn error_handling_metrics(findings: &[&dyn Finding]) -> ErrorHandlingMetrics {
    let mut metrics = ErrorHandlingMetrics::default();
    let mut chain_sites = HashSet::new();
    let mut compliance_sites = Vec::new();
    for finding in open_findings(findings) {
        if finding.rule().category() == "foreign_error_attenuation" {
            let handling = finding_field(finding, "handling_class").unwrap_or_default();
            match handling.as_str() {
                "ERROR-HANDLING-CHAIN-BREAK" => {
                    metrics.chain_breaks += 1;
                    chain_sites.insert(finding_site(finding));
                }
                "ERROR-HANDLING-PENDING-INFRA" => {
                    metrics.pending_infrastructure += 1;
                    chain_sites.insert(finding_site(finding));
                }
                "ERROR-HANDLING-NEUTRAL" => metrics.neutral += 1,
                _ => {}
            }
            continue;
        }
        if finding.rule().category() == "internal_error_chain"
            && finding.rule().id().contains("COMPLIANCE")
        {
            metrics.compliance += 1;
            compliance_sites.push(finding_site(finding));
        }
    }
    metrics.compliance_unique = compliance_sites
        .iter()
        .filter(|site| !chain_sites.contains(*site))
        .count();
    metrics.migration_backlog = metrics.chain_breaks + metrics.pending_infrastructure;
    metrics
}

#[instrument(level = "debug", skip(finding))]
fn finding_site(finding: &dyn Finding) -> (String, String) {
    (
        finding_field(finding, "file").unwrap_or_default(),
        finding_field(finding, "line").unwrap_or_default(),
    )
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct PanicMetrics {
    pub(super) checklist_total: usize,
    pub(super) panic: usize,
    pub(super) unreachable: usize,
    pub(super) expect: usize,
    pub(super) unwrap: usize,
    pub(super) compile_error: usize,
}

#[instrument(level = "debug", skip(findings))]
pub(super) fn panic_metrics(findings: &[&dyn Finding]) -> PanicMetrics {
    let mut metrics = PanicMetrics::default();
    for finding in open_findings(findings) {
        if finding.rule().category() != "panics" {
            continue;
        }
        if finding_field(finding, "checklist").as_deref() == Some("false") {
            continue;
        }
        metrics.checklist_total += 1;
        match finding_field(finding, "kind").unwrap_or_default().as_str() {
            "PANIC-SOURCE-PANIC" => metrics.panic += 1,
            "PANIC-SOURCE-UNREACHABLE" => metrics.unreachable += 1,
            "PANIC-SOURCE-EXPECT" => metrics.expect += 1,
            "PANIC-SOURCE-UNWRAP" => metrics.unwrap += 1,
            "PANIC-SOURCE-COMPILE-ERROR" => metrics.compile_error += 1,
            _ => {}
        }
    }
    metrics
}
