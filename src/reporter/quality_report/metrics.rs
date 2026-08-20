//! Open-finding counts for the quality-report areas.

use std::collections::HashSet;

use crate::objects::{Disposition, Finding, MapFindingSink};

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
            let handling = field(finding, "handling_class").unwrap_or_default();
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
        field(finding, "file").unwrap_or_default(),
        field(finding, "line").unwrap_or_default(),
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
        if field(finding, "checklist").as_deref() == Some("false") {
            continue;
        }
        metrics.checklist_total += 1;
        match field(finding, "kind").unwrap_or_default().as_str() {
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

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct DeriveMetrics {
    pub(super) total: usize,
    pub(super) builder: usize,
    pub(super) use_builder: usize,
    pub(super) getter: usize,
    pub(super) setter: usize,
    pub(super) as_ref: usize,
    pub(super) as_str: usize,
    pub(super) new: usize,
    pub(super) pub_field: usize,
}

#[instrument(level = "debug", skip(findings))]
pub(super) fn derive_metrics(findings: &[&dyn Finding]) -> DeriveMetrics {
    let mut metrics = DeriveMetrics::default();
    for finding in open_findings(findings) {
        if finding.rule().category() != "derives" {
            continue;
        }
        metrics.total += 1;
        match finding.rule().id() {
            "DERIVE-BUILDER-001" => metrics.builder += 1,
            "DERIVE-USE-BUILDER-001" => metrics.use_builder += 1,
            "DERIVE-GETTER-001" => metrics.getter += 1,
            "DERIVE-SETTER-001" => metrics.setter += 1,
            "DERIVE-ASREF-001" => metrics.as_ref += 1,
            "DERIVE-ASSTR-001" => metrics.as_str += 1,
            "DERIVE-NEW-001" => metrics.new += 1,
            "DERIVE-PUB-FIELD-001" => metrics.pub_field += 1,
            _ => {}
        }
    }
    metrics
}

#[derive(Debug, Default, Clone)]
pub(super) struct TracingMetrics {
    pub(super) gaps: usize,
    pub(super) suppressed: usize,
    pub(super) by_role: [usize; TRACING_ROLE_ORDER.len()],
}

const TRACING_ROLE_ORDER: [&str; 10] = [
    "constructor",
    "getter",
    "setter",
    "predicate",
    "scan",
    "io",
    "render",
    "trait_surface",
    "entry",
    "other",
];

#[instrument(level = "debug", skip(findings))]
pub(super) fn tracing_metrics(findings: &[&dyn Finding]) -> TracingMetrics {
    let mut metrics = TracingMetrics::default();
    for finding in findings {
        if finding.rule().category() != "tracing" {
            continue;
        }
        match finding.disposition() {
            Disposition::Open => {
                metrics.gaps += 1;
                let role = field(*finding, "role").unwrap_or_else(|| "other".to_string());
                let index = TRACING_ROLE_ORDER
                    .iter()
                    .position(|name| *name == role)
                    .unwrap_or(TRACING_ROLE_ORDER.len() - 1);
                metrics.by_role[index] += 1;
            }
            Disposition::Suppressed => metrics.suppressed += 1,
            Disposition::Exemplar => {}
        }
    }
    metrics
}

#[instrument(level = "debug", skip(metrics))]
pub(super) fn format_tracing_detail(metrics: &TracingMetrics) -> String {
    let roles = TRACING_ROLE_ORDER
        .iter()
        .zip(metrics.by_role)
        .filter(|(_, count)| *count > 0)
        .map(|(role, count)| format!("{role} **{count}**"))
        .collect::<Vec<_>>();
    if roles.is_empty() {
        format!(
            "**{}** open gaps, **{}** documented exceptions",
            metrics.gaps, metrics.suppressed
        )
    } else {
        format!(
            "**{}** open gaps ({}), **{}** documented exceptions",
            metrics.gaps,
            roles.join(", "),
            metrics.suppressed
        )
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct ModularityMetrics {
    pub(super) inventory_total: usize,
    pub(super) checklist_total: usize,
    pub(super) large_files: usize,
    pub(super) large_functions: usize,
    pub(super) types_per_file: usize,
    pub(super) module_outliers: usize,
    pub(super) top_heavy: usize,
    pub(super) lopsided: usize,
    pub(super) collapse: usize,
}

#[instrument(level = "debug", skip(findings))]
pub(super) fn modularity_metrics(findings: &[&dyn Finding]) -> ModularityMetrics {
    let mut metrics = ModularityMetrics::default();
    for finding in open_findings(findings) {
        if finding.rule().category() != "modularity" {
            continue;
        }
        if field(finding, "kind").as_deref() == Some("MODULARITY-FUNCTION") {
            let lines = field(finding, "lines")
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(0);
            if lines < crate::config::ModularityThresholds::default().function_inventory_min_lines() {
                continue;
            }
        }
        metrics.inventory_total += 1;
        if field(finding, "checklist").as_deref() != Some("true") {
            continue;
        }
        metrics.checklist_total += 1;
        match field(finding, "kind").unwrap_or_default().as_str() {
            "MODULARITY-FILE" => metrics.large_files += 1,
            "MODULARITY-FUNCTION" => metrics.large_functions += 1,
            "MODULARITY-TYPES-PER-FILE" => metrics.types_per_file += 1,
            "MODULARITY-MODULE-SIZE" => metrics.module_outliers += 1,
            "MODULARITY-TOP-HEAVY" => metrics.top_heavy += 1,
            "MODULARITY-LOPSIDED" => metrics.lopsided += 1,
            "MODULARITY-COLLAPSE" => metrics.collapse += 1,
            _ => {}
        }
    }
    metrics
}

#[instrument(level = "debug", skip(findings))]
pub(super) fn count_open_category(findings: &[&dyn Finding], category: &str) -> usize {
    open_findings(findings)
        .filter(|finding| finding.rule().category() == category)
        .count()
}

#[instrument(level = "debug", skip(findings))]
pub(super) fn count_open_rule(findings: &[&dyn Finding], rule_id: &str) -> usize {
    open_findings(findings)
        .filter(|finding| finding.rule().id() == rule_id)
        .count()
}

#[instrument(level = "debug", skip(findings))]
fn open_findings<'a>(
    findings: &'a [&'a dyn Finding],
) -> impl Iterator<Item = &'a dyn Finding> + 'a {
    findings
        .iter()
        .copied()
        .filter(|finding| finding.disposition() == Disposition::Open)
}

#[instrument(level = "debug", skip(finding))]
fn field(finding: &dyn Finding, name: &str) -> Option<String> {
    let mut sink = MapFindingSink::default();
    finding.emit(&mut sink);
    sink.fields
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}
