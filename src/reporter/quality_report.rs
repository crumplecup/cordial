use std::collections::HashSet;
use std::fmt::Write as _;

use crate::error::CordialResult;
use crate::hooks::Reporter;
use crate::ir::IrView;
use crate::objects::{Artifact, Disposition, Finding, MapFindingSink, TextArtifact};
use crate::session::SessionView;

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

    let allows = count_open_category(findings, "allows");
    let tracing = tracing_metrics(findings);
    let modularity = modularity_metrics(findings);

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
            title: "Derive patterns",
            open_items: derives.total,
            checklist: "derives.checklist.md",
            summary: "derives-summary.md",
            detail: derive_detail,
        },
        QualityAreaSummary {
            priority: 3,
            title: "Allow attributes",
            open_items: allows,
            checklist: "allows.checklist.md",
            summary: "allows-summary.md",
            detail: format!("allow attributes **{allows}**"),
        },
        QualityAreaSummary {
            priority: 4,
            title: "Tracing instrumentation",
            open_items: tracing.gaps,
            checklist: "tracing-instrument.checklist.md",
            summary: "tracing-summary.md",
            detail: format_tracing_detail(&tracing),
        },
        QualityAreaSummary {
            priority: 5,
            title: "Modularity",
            open_items: modularity.checklist_total,
            checklist: "modularity.checklist.md",
            summary: "modularity-summary.md",
            detail: format!(
                "large files **{}**, large functions **{}**, types-per-file **{}**, \
                 module-size outliers **{}**, top-heavy **{}**, lopsided **{}** \
                 (checklist cutoffs; **{}** inventory rows tracked in CSV)",
                modularity.large_files,
                modularity.large_functions,
                modularity.types_per_file,
                modularity.module_outliers,
                modularity.top_heavy,
                modularity.lopsided,
                modularity.inventory_total,
            ),
        },
    ];

    let total_open_items = areas.iter().map(|area| area.open_items).sum();

    Ok(QualityReport {
        areas,
        total_open_items,
    })
}

pub fn render_quality_report_markdown(report: &QualityReport) -> CordialResult<String> {
    let mut out = String::new();
    writeln!(out, "# Code quality report")?;
    writeln!(out, "\n**Total open items:** {}\n", report.total_open_items)?;
    writeln!(
        out,
        "Resolve issues in the order below. Each area has a checklist (action items) \
         and a summary (workspace rollup). Error-handling detail also lives in \
         `panics.checklist.md`, `error-sites.checklist.md`, \
         `error-chain-preserved.checklist.md`, `internal-error-chain.checklist.md`, \
         `foreign-error-types.checklist.md`, and \
         `foreign-error-attenuation.checklist.md`. `Box<dyn Error>` and \
         `Result<_, String>` from `antipatterns.checklist.md` roll into this \
         area's open count.\n"
    )?;

    writeln!(out, "## Resolution order\n")?;
    writeln!(
        out,
        "| Priority | Area | Open items | Checklist | Summary |"
    )?;
    writeln!(out, "| ---: | --- | ---: | --- | --- |")?;
    for area in &report.areas {
        writeln!(
            out,
            "| {} | {} | {} | [`{}`]({}) | [`{}`]({}) |",
            area.priority,
            area.title,
            area.open_items,
            area.checklist,
            area.checklist,
            area.summary,
            area.summary,
        )?;
    }
    writeln!(out)?;

    for area in &report.areas {
        writeln!(out, "## {}. {}\n", area.priority, area.title)?;
        writeln!(out, "**Open items:** {}\n", area.open_items)?;
        writeln!(out, "_{}_\n", area.detail)?;
        writeln!(
            out,
            "- Checklist: [`{}`]({})\n- Summary: [`{}`]({})\n",
            area.checklist, area.checklist, area.summary, area.summary,
        )?;
    }

    Ok(out)
}

pub fn render_quality_workspace_summary_markdown(report: &QualityReport) -> CordialResult<String> {
    let mut out = String::new();
    writeln!(out, "# Quality workspace summary")?;
    writeln!(out, "\n**Total open items:** {}\n", report.total_open_items)?;
    writeln!(
        out,
        "Bird's-eye view after running quality heuristics on the workspace. \
         Per-heuristic rollups live in `*-summary.md` files; resolution priority and \
         area notes are in [`quality-report.md`](quality-report.md).\n"
    )?;

    writeln!(out, "## Heuristics\n")?;
    writeln!(
        out,
        "| Priority | Heuristic | Open items | Checklist | Rollup | Notes |"
    )?;
    writeln!(out, "| --- | --- | ---: | --- | --- | --- |")?;
    for area in &report.areas {
        writeln!(
            out,
            "| {} | {} | {} | [`{}`]({}) | [`{}`]({}) | {} |",
            area.priority,
            area.title,
            area.open_items,
            area.checklist,
            area.checklist,
            area.summary,
            area.summary,
            area.detail,
        )?;
    }
    writeln!(out)?;

    Ok(out)
}

/// Writes `quality-report.md` and `summary.md` after a quality session.
#[derive(Debug, Default, Clone, Copy)]
pub struct QualityReportReporter;

impl QualityReportReporter {
    pub const ID: &'static str = "quality-report";
}

impl Reporter for QualityReportReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(
        &self,
        findings: &[&dyn Finding],
        _ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
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

#[derive(Debug, Default, Clone, Copy)]
struct ErrorHandlingMetrics {
    chain_breaks: usize,
    pending_infrastructure: usize,
    neutral: usize,
    compliance: usize,
    compliance_unique: usize,
    migration_backlog: usize,
}

fn error_handling_metrics(findings: &[&dyn Finding]) -> ErrorHandlingMetrics {
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

fn finding_site(finding: &dyn Finding) -> (String, String) {
    (
        field(finding, "file").unwrap_or_default(),
        field(finding, "line").unwrap_or_default(),
    )
}

#[derive(Debug, Default, Clone, Copy)]
struct PanicMetrics {
    checklist_total: usize,
    panic: usize,
    unreachable: usize,
    expect: usize,
    unwrap: usize,
    compile_error: usize,
}

fn panic_metrics(findings: &[&dyn Finding]) -> PanicMetrics {
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
struct DeriveMetrics {
    total: usize,
    builder: usize,
    getter: usize,
    setter: usize,
    new: usize,
    pub_field: usize,
}

fn derive_metrics(findings: &[&dyn Finding]) -> DeriveMetrics {
    let mut metrics = DeriveMetrics::default();
    for finding in open_findings(findings) {
        if finding.rule().category() != "derives" {
            continue;
        }
        metrics.total += 1;
        match finding.rule().id() {
            "DERIVE-BUILDER-001" => metrics.builder += 1,
            "DERIVE-GETTER-001" => metrics.getter += 1,
            "DERIVE-SETTER-001" => metrics.setter += 1,
            "DERIVE-NEW-001" => metrics.new += 1,
            "DERIVE-PUB-FIELD-001" => metrics.pub_field += 1,
            _ => {}
        }
    }
    metrics
}

#[derive(Debug, Default, Clone)]
struct TracingMetrics {
    gaps: usize,
    suppressed: usize,
    by_role: [usize; TRACING_ROLE_ORDER.len()],
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

fn tracing_metrics(findings: &[&dyn Finding]) -> TracingMetrics {
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

fn format_tracing_detail(metrics: &TracingMetrics) -> String {
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
struct ModularityMetrics {
    inventory_total: usize,
    checklist_total: usize,
    large_files: usize,
    large_functions: usize,
    types_per_file: usize,
    module_outliers: usize,
    top_heavy: usize,
    lopsided: usize,
}

fn modularity_metrics(findings: &[&dyn Finding]) -> ModularityMetrics {
    let mut metrics = ModularityMetrics::default();
    for finding in open_findings(findings) {
        if finding.rule().category() != "modularity" {
            continue;
        }
        if field(finding, "kind").as_deref() == Some("MODULARITY-FUNCTION") {
            let lines = field(finding, "lines")
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(0);
            if lines < crate::config::ModularityThresholds::default().function_inventory_min_lines {
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
            _ => {}
        }
    }
    metrics
}

fn count_open_category(findings: &[&dyn Finding], category: &str) -> usize {
    open_findings(findings)
        .filter(|finding| finding.rule().category() == category)
        .count()
}

fn count_open_rule(findings: &[&dyn Finding], rule_id: &str) -> usize {
    open_findings(findings)
        .filter(|finding| finding.rule().id() == rule_id)
        .count()
}

fn open_findings<'a>(
    findings: &'a [&'a dyn Finding],
) -> impl Iterator<Item = &'a dyn Finding> + 'a {
    findings
        .iter()
        .copied()
        .filter(|finding| finding.disposition() == Disposition::Open)
}

fn field(finding: &dyn Finding, name: &str) -> Option<String> {
    let mut sink = MapFindingSink::default();
    finding.emit(&mut sink);
    sink.fields
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}
