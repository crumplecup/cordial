//! This etiquette's own `quality-report.md` rollup contribution.

use crate::etiquette::{finding_field, open_findings};
use crate::objects::Finding;

use tracing::instrument;

#[derive(Debug, Default, Clone, Copy)]
struct Metrics {
    inventory_total: usize,
    checklist_total: usize,
    large_files: usize,
    large_functions: usize,
    types_per_file: usize,
    module_outliers: usize,
    top_heavy: usize,
    lopsided: usize,
    collapse: usize,
}

#[instrument(level = "debug", skip(findings))]
fn metrics(findings: &[&dyn Finding]) -> Metrics {
    let mut metrics = Metrics::default();
    for finding in open_findings(findings) {
        if finding.rule().category() != "modularity" {
            continue;
        }
        if finding_field(finding, "kind").as_deref() == Some("MODULARITY-FUNCTION") {
            let lines = finding_field(finding, "lines")
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(0);
            if lines < crate::config::ModularityThresholds::default().function_inventory_min_lines()
            {
                continue;
            }
        }
        metrics.inventory_total += 1;
        if finding_field(finding, "checklist").as_deref() != Some("true") {
            continue;
        }
        metrics.checklist_total += 1;
        match finding_field(finding, "kind").unwrap_or_default().as_str() {
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
pub(super) fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let metrics = metrics(findings);
    let detail = format!(
        "large files **{}**, large functions **{}**, types-per-file **{}**, \
         module-size outliers **{}**, top-heavy **{}**, lopsided **{}**, \
         collapse **{}** (checklist cutoffs; **{}** inventory rows tracked in CSV)",
        metrics.large_files,
        metrics.large_functions,
        metrics.types_per_file,
        metrics.module_outliers,
        metrics.top_heavy,
        metrics.lopsided,
        metrics.collapse,
        metrics.inventory_total,
    );
    (metrics.checklist_total, detail)
}
