//! This etiquette's own `quality-report.md` rollup contribution.

use crate::etiquette::finding_field;
use crate::objects::{Disposition, Finding};

use super::print::PrintRuleId;
use super::subscriber::SubscriberRuleId;

use tracing::instrument;

const ROLE_ORDER: [&str; 10] = [
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

#[derive(Debug, Default, Clone)]
struct Metrics {
    gaps: usize,
    subscriber: usize,
    std_print: usize,
    suppressed: usize,
    by_role: [usize; ROLE_ORDER.len()],
}

#[instrument(level = "debug", skip(findings))]
fn metrics(findings: &[&dyn Finding]) -> Metrics {
    let mut metrics = Metrics::default();
    for finding in findings {
        if finding.rule().category() != "tracing" {
            continue;
        }
        if SubscriberRuleId::is_subscriber_rule(finding.rule().id()) {
            match finding.disposition() {
                Disposition::Open => {
                    metrics.gaps += 1;
                    metrics.subscriber += 1;
                }
                Disposition::Suppressed => metrics.suppressed += 1,
                Disposition::Exemplar => {}
            }
            continue;
        }
        if PrintRuleId::is_print_rule(finding.rule().id()) {
            match finding.disposition() {
                Disposition::Open => {
                    metrics.gaps += 1;
                    metrics.std_print += 1;
                }
                Disposition::Suppressed => metrics.suppressed += 1,
                Disposition::Exemplar => {}
            }
            continue;
        }
        match finding.disposition() {
            Disposition::Open => {
                metrics.gaps += 1;
                let role = finding_field(*finding, "role").unwrap_or_else(|| "other".to_string());
                let index = ROLE_ORDER
                    .iter()
                    .position(|name| *name == role)
                    .unwrap_or(ROLE_ORDER.len() - 1);
                metrics.by_role[index] += 1;
            }
            Disposition::Suppressed => metrics.suppressed += 1,
            Disposition::Exemplar => {}
        }
    }
    metrics
}

#[instrument(level = "debug", skip(findings))]
pub(super) fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let metrics = metrics(findings);
    let mut parts: Vec<String> = ROLE_ORDER
        .iter()
        .zip(metrics.by_role)
        .filter(|(_, count)| *count > 0)
        .map(|(role, count)| format!("{role} **{count}**"))
        .collect();
    if metrics.subscriber > 0 {
        parts.push(format!("subscriber **{}**", metrics.subscriber));
    }
    if metrics.std_print > 0 {
        parts.push(format!("std print **{}**", metrics.std_print));
    }
    let detail = if parts.is_empty() {
        format!(
            "**{}** open gaps, **{}** documented exceptions",
            metrics.gaps, metrics.suppressed
        )
    } else {
        format!(
            "**{}** open gaps ({}), **{}** documented exceptions",
            metrics.gaps,
            parts.join(", "),
            metrics.suppressed
        )
    };
    (metrics.gaps, detail)
}
