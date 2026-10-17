mod metrics;
mod render;

use std::fmt::Write as _;

use crate::error::CordialResult;
use crate::etiquette::{QualityChecklistSpec, count_open_rule};
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, TextArtifact};

use tracing::instrument;

use metrics::{error_handling_metrics, panic_metrics};

pub use render::{render_quality_report_markdown, render_quality_workspace_summary_markdown};

/// One resolution-priority area in the code quality report.
#[derive(Debug, Clone, PartialEq, Eq, derive_new::new, derive_getters::Getters)]
pub struct QualityAreaSummary {
    /// Sort key; lower numbers appear first in the report.
    #[getter(copy)]
    priority: u8,
    /// Section title in `quality-report.md`.
    title: String,
    /// Open findings in this area.
    #[getter(copy)]
    open_items: usize,
    /// Checklist artifact filename.
    checklist: String,
    /// Checklist artifacts and their owned rule sections in worklist order.
    checklists: Vec<QualityChecklist>,
    /// Summary artifact filename.
    summary: String,
    /// One-line breakdown shown in the rollup table.
    detail: String,
}

/// A checklist file, optionally restricted to specific rule headings.
#[derive(Debug, Clone, PartialEq, Eq, derive_new::new, derive_getters::Getters)]
pub struct QualityChecklist {
    /// Artifact filename under `findings/`.
    name: String,
    /// Rule headings included from the artifact; empty means all open items.
    rules: Vec<String>,
}

impl QualityChecklist {
    /// Whether an item under `rule` belongs to this area.
    #[instrument(level = "trace", skip(self))]
    pub fn includes_rule(&self, rule: &str) -> bool {
        self.rules.is_empty() || self.rules.iter().any(|included| included == rule)
    }
}

/// Workspace code quality report in resolution order.
#[derive(Debug, Clone, PartialEq, Eq, derive_new::new, derive_getters::Getters)]
pub struct QualityReport {
    /// Per-etiquette sections of the quality report.
    areas: Vec<QualityAreaSummary>,
    /// Open findings across every area.
    #[getter(copy)]
    total_open_items: usize,
}

/// Build the unified report from session findings (open items only).
#[instrument(level = "debug", skip(findings), err(level = "warn"))]
pub fn build_quality_report(findings: &[&dyn Finding]) -> CordialResult<QualityReport> {
    let error = error_handling_metrics(findings);
    let box_dyn_error = count_open_rule(findings, "ANTIPATTERN-BOX-DYN-ERROR-001");
    let string_error = count_open_rule(findings, "ANTIPATTERN-STRING-ERROR-001");
    let panics = panic_metrics(findings);
    let error_open = error.migration_backlog()
        + error.compliance_unique()
        + box_dyn_error
        + string_error
        + panics.checklist_total();

    let mut error_detail = format!(
        "migration backlog **{}** (chain breaks **{}** + pending infra **{}**), \
         internal compliance **{}** (**{}** unique vs chain sites), \
         `Box<dyn Error>` **{box_dyn_error}**, `Result<_, String>` **{string_error}**, \
         abort-site action items **{}** (panic **{}**, unwrap **{}**, expect **{}**; \
         library → wrap associated errors, binary/tests → miette)",
        error.migration_backlog(),
        error.chain_breaks(),
        error.pending_infrastructure(),
        error.compliance(),
        error.compliance_unique(),
        panics.checklist_total(),
        panics.panic(),
        panics.unwrap(),
        panics.expect(),
    );
    if error.neutral() > 0 {
        write!(error_detail, ", manual review **{}**", error.neutral())?;
    }

    let mut areas = vec![quality_area(
        1,
        "Error handling",
        error_open,
        "panics.checklist.md",
        &[],
        &[
            QualityChecklistSpec::new("internal-error-chain.checklist.md"),
            QualityChecklistSpec::new("foreign-error-attenuation.checklist.md").with_rules(&[
                "Chain break (bad code → migrate)",
                "Pending infrastructure (good code after `From` wiring)",
            ]),
            QualityChecklistSpec::new("antipatterns.checklist.md").with_rules(&[
                "ANTIPATTERN-BOX-DYN-ERROR-001",
                "ANTIPATTERN-STRING-ERROR-001",
            ]),
        ],
        "foreign-error-attenuation-summary.md",
        error_detail,
    )];

    // Remaining areas come from each quality etiquette's quality_area(),
    // ordered by LintOrder: the resolution chain from foreign error types
    // through proof patterns.
    let contributing: Vec<_> = crate::etiquettes::quality_report_etiquettes()
        .into_iter()
        .filter(|etiquette| etiquette.quality_area().is_some())
        .collect();
    let ordered = crate::etiquette::sort_quality_etiquettes(&contributing, &crate::BUILT_IN_ORDER);
    let mut priority = 2u8;
    for etiquette in ordered {
        let Some(spec) = etiquette.quality_area() else {
            continue;
        };
        let (open_items, detail) = (spec.compute())(findings);
        areas.push(quality_area(
            priority,
            spec.title(),
            open_items,
            spec.checklist(),
            spec.checklist_rules(),
            spec.extra_checklists(),
            spec.summary(),
            detail,
        ));
        priority += 1;
    }

    let total_open_items = areas.iter().map(QualityAreaSummary::open_items).sum();

    Ok(QualityReport::new(areas, total_open_items))
}

#[instrument(level = "debug", skip(extra_checklists))]
fn quality_area(
    priority: u8,
    title: &str,
    open_items: usize,
    checklist: &str,
    checklist_rules: &[&str],
    extra_checklists: &[QualityChecklistSpec],
    summary: &str,
    detail: String,
) -> QualityAreaSummary {
    let mut checklists = vec![QualityChecklist::new(
        checklist.to_string(),
        checklist_rules
            .iter()
            .map(|rule| (*rule).to_string())
            .collect(),
    )];
    checklists.extend(extra_checklists.iter().map(|source| {
        QualityChecklist::new(
            source.name().to_string(),
            source
                .rules()
                .iter()
                .map(|rule| (*rule).to_string())
                .collect(),
        )
    }));
    QualityAreaSummary::new(
        priority,
        title.to_string(),
        open_items,
        checklist.to_string(),
        checklists,
        summary.to_string(),
        detail,
    )
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
        let findings = view.findings();

        let report = build_quality_report(findings)?;
        Ok(vec![
            Box::new(TextArtifact::new(
                "quality-report.md".to_string(),
                "text/markdown".to_string(),
                render_quality_report_markdown(&report)?,
            )),
            Box::new(TextArtifact::new(
                "summary.md".to_string(),
                "text/markdown".to_string(),
                render_quality_workspace_summary_markdown(&report)?,
            )),
        ])
    }
}
