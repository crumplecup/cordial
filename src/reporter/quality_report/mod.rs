mod metrics;
mod render;

use std::fmt::Write as _;

use crate::error::CordialResult;
use crate::etiquette::count_open_rule;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, TextArtifact};

use tracing::instrument;

use metrics::{error_handling_metrics, panic_metrics};

pub use render::{render_quality_report_markdown, render_quality_workspace_summary_markdown};

/// One resolution-priority area in the code quality report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityAreaSummary {
    /// Sort key; lower numbers appear first in the report.
    pub priority: u8,
    /// Section title in `quality-report.md`.
    pub title: String,
    /// Open findings in this area.
    pub open_items: usize,
    /// Checklist artifact filename.
    pub checklist: String,
    /// Summary artifact filename.
    pub summary: String,
    /// One-line breakdown shown in the rollup table.
    pub detail: String,
}

/// Workspace code quality report in resolution order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityReport {
    /// Per-etiquette sections of the quality report.
    pub areas: Vec<QualityAreaSummary>,
    /// Open findings across every area.
    pub total_open_items: usize,
}

/// Build the unified report from session findings (open items only).
#[instrument(level = "debug", skip(findings), err(level = "warn"))]
pub fn build_quality_report(findings: &[&dyn Finding]) -> CordialResult<QualityReport> {
    let error = error_handling_metrics(findings);
    let box_dyn_error = count_open_rule(findings, "ANTIPATTERN-BOX-DYN-ERROR-001");
    let string_error = count_open_rule(findings, "ANTIPATTERN-STRING-ERROR-001");
    let panics = panic_metrics(findings);
    let error_open = error.migration_backlog
        + error.compliance_unique
        + box_dyn_error
        + string_error
        + panics.checklist_total;

    let mut error_detail = format!(
        "migration backlog **{}** (chain breaks **{}** + pending infra **{}**), \
         internal compliance **{}** (**{}** unique vs chain sites), \
         `Box<dyn Error>` **{box_dyn_error}**, `Result<_, String>` **{string_error}**, \
         abort-site action items **{}** (panic **{}**, unwrap **{}**, expect **{}**; \
         library → wrap associated errors, binary/tests → miette)",
        error.migration_backlog,
        error.chain_breaks,
        error.pending_infrastructure,
        error.compliance,
        error.compliance_unique,
        panics.checklist_total,
        panics.panic,
        panics.unwrap,
        panics.expect,
    );
    if error.neutral > 0 {
        write!(error_detail, ", manual review **{}**", error.neutral)?;
    }

    let mut areas = vec![quality_area(
        1,
        "Error handling",
        error_open,
        "panics.checklist.md",
        "foreign-error-attenuation-summary.md",
        error_detail,
    )];

    // Every other area comes from each quality etiquette's own
    // QualityReportArea::quality_area() -- see crate::etiquettes::
    // quality_report_areas(). An etiquette that declines (None) simply
    // contributes no row; one that's missing from that registry entirely
    // doesn't compile into quality_etiquettes() either, so there is no
    // way for a registered etiquette to be silently absent from this
    // report.
    let mut priority = 2u8;
    for etiquette in crate::etiquettes::quality_report_areas() {
        let Some(spec) = etiquette.quality_area() else {
            continue;
        };
        let (open_items, detail) = (spec.compute)(findings);
        areas.push(quality_area(
            priority,
            spec.title,
            open_items,
            spec.checklist,
            spec.summary,
            detail,
        ));
        priority += 1;
    }

    let total_open_items = areas.iter().map(|area| area.open_items).sum();

    Ok(QualityReport {
        areas,
        total_open_items,
    })
}

#[instrument(level = "trace", skip(detail))]
fn quality_area(
    priority: u8,
    title: &str,
    open_items: usize,
    checklist: &str,
    summary: &str,
    detail: String,
) -> QualityAreaSummary {
    QualityAreaSummary {
        priority,
        title: title.to_string(),
        open_items,
        checklist: checklist.to_string(),
        summary: summary.to_string(),
        detail,
    }
}

/// Writes `quality-report.md` and `summary.md` after a quality session.
#[derive(Debug, Default, Clone, Copy)]
pub struct QualityReportReporter;

impl QualityReportReporter {
    /// Stable identifier for `QualityReportReporter`.
    pub const ID: &'static str = "quality-report";
}

impl Reporter for QualityReportReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let report = build_quality_report(findings)?;
        Ok(vec![
            Box::new(TextArtifact {
                name: "quality-report.md".to_string(),
                media_type: "text/markdown".to_string(),
                body: render_quality_report_markdown(&report)?,
            }),
            Box::new(TextArtifact {
                name: "summary.md".to_string(),
                media_type: "text/markdown".to_string(),
                body: render_quality_workspace_summary_markdown(&report)?,
            }),
        ])
    }
}
