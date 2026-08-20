mod metrics;
mod render;

use std::fmt::Write as _;

use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, TextArtifact};

use tracing::instrument;

use metrics::{
    count_open_category, count_open_rule, derive_metrics, error_handling_metrics,
    format_tracing_detail, modularity_metrics, panic_metrics, tracing_metrics,
};

pub use render::{render_quality_report_markdown, render_quality_workspace_summary_markdown};

/// One resolution-priority area in the code quality report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityAreaSummary {
    pub priority: u8,
    pub title: String,
    pub open_items: usize,
    pub checklist: String,
    pub summary: String,
    pub detail: String,
}

/// Workspace code quality report in resolution order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityReport {
    pub areas: Vec<QualityAreaSummary>,
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

    let derives = derive_metrics(findings);
    let derive_detail = format!(
        "builder **{}**, use_builder **{}**, getter **{}**, setter **{}**, as_ref **{}**, as_str **{}**, new **{}**, pub_field **{}**",
        derives.builder,
        derives.use_builder,
        derives.getter,
        derives.setter,
        derives.as_ref,
        derives.as_str,
        derives.new,
        derives.pub_field,
    );

    let unused_arg = count_open_rule(findings, "ANTIPATTERN-UNUSED-UNDERSCORE-ARG-001");
    let static_ref = count_open_rule(findings, "ANTIPATTERN-STRUCT-STATIC-REF-001");
    let version_in_member = count_open_rule(findings, "ANTIPATTERN-VERSION-IN-MEMBER-001");
    let unnamed_contract = count_open_rule(findings, "ANTIPATTERN-UNNAMED-CONTRACT-BOUND-001");
    let antipatterns = unused_arg + static_ref + version_in_member + unnamed_contract;

    let allows = count_open_category(findings, "allows");
    let tracing = tracing_metrics(findings);
    let modularity = modularity_metrics(findings);

    let glob_imports = count_open_category(findings, "glob_imports");
    let inline_tests = count_open_category(findings, "inline_tests");
    let visibility_flat = count_open_rule(findings, "VIS-CRATE-FLAT-001");
    let visibility_thin = count_open_rule(findings, "VIS-MOD-THIN-001");
    let visibility_mismatch = count_open_rule(findings, "VIS-MOD-MISMATCH-001");
    let visibility = visibility_flat + visibility_thin + visibility_mismatch;
    let cfg_scatter = count_open_category(findings, "cfg_scatter");
    let cli_island = count_open_rule(findings, "CLI-ISLAND-001");
    let cli_act = count_open_rule(findings, "CLI-ACT-001");
    let cli_main = count_open_rule(findings, "CLI-MAIN-001");
    let cli_layout = cli_island + cli_act + cli_main;

    let areas = vec![
        quality_area(
            1,
            "Error handling",
            error_open,
            "panics.checklist.md",
            "foreign-error-attenuation-summary.md",
            error_detail,
        ),
        quality_area(
            2,
            "Antipatterns",
            antipatterns,
            "antipatterns.checklist.md",
            "antipatterns-summary.md",
            format!(
                "unused `_arg` **{unused_arg}**, static refs **{static_ref}**, \
                 version-in-member **{version_in_member}**, unnamed contract **{unnamed_contract}**"
            ),
        ),
        quality_area(
            3,
            "Derive patterns",
            derives.total,
            "derives.checklist.md",
            "derives-summary.md",
            derive_detail,
        ),
        quality_area(
            4,
            "Allow attributes",
            allows,
            "allows.checklist.md",
            "allows-summary.md",
            format!("allow attributes **{allows}**"),
        ),
        quality_area(
            5,
            "Tracing instrumentation",
            tracing.gaps,
            "tracing-instrument.checklist.md",
            "tracing-summary.md",
            format_tracing_detail(&tracing),
        ),
        quality_area(
            6,
            "Modularity",
            modularity.checklist_total,
            "modularity.checklist.md",
            "modularity-summary.md",
            format!(
                "large files **{}**, large functions **{}**, types-per-file **{}**, \
                 module-size outliers **{}**, top-heavy **{}**, lopsided **{}**, \
                 collapse **{}** (checklist cutoffs; **{}** inventory rows tracked in CSV)",
                modularity.large_files,
                modularity.large_functions,
                modularity.types_per_file,
                modularity.module_outliers,
                modularity.top_heavy,
                modularity.lopsided,
                modularity.collapse,
                modularity.inventory_total,
            ),
        ),
        quality_area(
            7,
            "Module visibility",
            visibility,
            "visibility.checklist.md",
            "visibility-summary.md",
            format!(
                "crate-flat **{visibility_flat}**, thin-mod **{visibility_thin}**, \
                 vis-mismatch **{visibility_mismatch}**"
            ),
        ),
        quality_area(
            8,
            "Cfg scatter",
            cfg_scatter,
            "cfg-scatter.checklist.md",
            "cfg-scatter-summary.md",
            format!("scattered `#[cfg]` groups **{cfg_scatter}**"),
        ),
        quality_area(
            9,
            "CLI layout",
            cli_layout,
            "cli-layout.checklist.md",
            "cli-layout-summary.md",
            format!("island **{cli_island}**, act **{cli_act}**, main **{cli_main}**"),
        ),
        quality_area(
            10,
            "Glob imports",
            glob_imports,
            "glob-imports.checklist.md",
            "glob-imports-summary.md",
            format!("glob `use` sites **{glob_imports}**"),
        ),
        quality_area(
            11,
            "Inline tests",
            inline_tests,
            "inline-tests.checklist.md",
            "inline-tests-summary.md",
            format!("tests under `src/` **{inline_tests}**"),
        ),
    ];

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
