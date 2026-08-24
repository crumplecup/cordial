//! This etiquette's own `quality-report.md` rollup contribution.

use crate::etiquette::finding_field;
use crate::objects::{Disposition, Finding};

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
    let roles = ROLE_ORDER
        .iter()
        .zip(metrics.by_role)
        .filter(|(_, count)| *count > 0)
        .map(|(role, count)| format!("{role} **{count}**"))
        .collect::<Vec<_>>();
    let detail = if roles.is_empty() {
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
    };
    (metrics.gaps, detail)
}
