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
    pub title: &'static str,
    pub open_items: usize,
    pub checklist: &'static str,
    pub summary: &'static str,
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
        "builder **{}**, getter **{}**, setter **{}**, new **{}**, pub_field **{}**",
        derives.builder, derives.getter, derives.setter, derives.new, derives.pub_field,
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
        QualityAreaSummary {
            priority: 1,
            title: "Error handling",
            open_items: error_open,
            checklist: "panics.checklist.md",
            summary: "foreign-error-attenuation-summary.md",
            detail: error_detail,
        },
        QualityAreaSummary {
            priority: 2,
            title: "Antipatterns",
            open_items: antipatterns,
            checklist: "antipatterns.checklist.md",
            summary: "antipatterns-summary.md",
            detail: format!(
                "unused `_arg` **{unused_arg}**, static refs **{static_ref}**, \
                 version-in-member **{version_in_member}**, unnamed contract **{unnamed_contract}**"
            ),
        },
        QualityAreaSummary {
            priority: 3,
            title: "Derive patterns",
            open_items: derives.total,
            checklist: "derives.checklist.md",
            summary: "derives-summary.md",
            detail: derive_detail,
        },
        QualityAreaSummary {
            priority: 4,
            title: "Allow attributes",
            open_items: allows,
            checklist: "allows.checklist.md",
            summary: "allows-summary.md",
            detail: format!("allow attributes **{allows}**"),
        },
        QualityAreaSummary {
            priority: 5,
            title: "Tracing instrumentation",
            open_items: tracing.gaps,
            checklist: "tracing-instrument.checklist.md",
            summary: "tracing-summary.md",
            detail: format_tracing_detail(&tracing),
        },
        QualityAreaSummary {
            priority: 6,
            title: "Modularity",
            open_items: modularity.checklist_total,
            checklist: "modularity.checklist.md",
            summary: "modularity-summary.md",
            detail: format!(
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
        },
        QualityAreaSummary {
            priority: 7,
            title: "Module visibility",
            open_items: visibility,
            checklist: "visibility.checklist.md",
            summary: "visibility-summary.md",
            detail: format!(
                "crate-flat **{visibility_flat}**, thin-mod **{visibility_thin}**, \
                 vis-mismatch **{visibility_mismatch}**"
            ),
        },
        QualityAreaSummary {
            priority: 8,
            title: "Cfg scatter",
            open_items: cfg_scatter,
            checklist: "cfg-scatter.checklist.md",
            summary: "cfg-scatter-summary.md",
            detail: format!("scattered `#[cfg]` groups **{cfg_scatter}**"),
        },
        QualityAreaSummary {
            priority: 9,
            title: "CLI layout",
            open_items: cli_layout,
            checklist: "cli-layout.checklist.md",
            summary: "cli-layout-summary.md",
            detail: format!("island **{cli_island}**, act **{cli_act}**, main **{cli_main}**"),
        },
        QualityAreaSummary {
            priority: 10,
            title: "Glob imports",
            open_items: glob_imports,
            checklist: "glob-imports.checklist.md",
            summary: "glob-imports-summary.md",
            detail: format!("glob `use` sites **{glob_imports}**"),
        },
        QualityAreaSummary {
            priority: 11,
            title: "Inline tests",
            open_items: inline_tests,
            checklist: "inline-tests.checklist.md",
            summary: "inline-tests-summary.md",
            detail: format!("tests under `src/` **{inline_tests}**"),
        },
    ];

    let total_open_items = areas.iter().map(|area| area.open_items).sum();

    Ok(QualityReport {
        areas,
        total_open_items,
    })
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
