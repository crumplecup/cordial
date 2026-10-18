//! Markdown rendering for the unified quality report.

use std::fmt::Write as _;

use crate::error::CordialResult;

use super::{QualityAreaSummary, QualityReport, ReportRunCoverage};

use tracing::instrument;

/// Render quality report markdown.
#[instrument(level = "debug", skip(report), err(level = "warn"))]
pub fn render_quality_report_markdown(report: &QualityReport) -> CordialResult<String> {
    render_quality_report_with_coverage(report, None)
}

#[instrument(level = "debug", skip(report, coverage))]
pub(super) fn render_quality_report_with_coverage(
    report: &QualityReport,
    coverage: Option<&ReportRunCoverage>,
) -> CordialResult<String> {
    let mut out = String::new();
    writeln!(out, "# Code quality report")?;
    writeln!(
        out,
        "\n**Total open items:** {}\n",
        report.total_open_items()
    )?;
    if coverage.is_some() {
        writeln!(
            out,
            "_Total includes only checks run now; skipped areas retain their last full-run count below._\n"
        )?;
    }
    writeln!(
        out,
        "Resolve issues in the order below. Each area has a checklist (action items) \
         and a summary (workspace rollup). Error-handling detail also lives in \
         `panics.checklist.md`, `error-sites.checklist.md`, \
         `error-chain-preserved.checklist.md`, `internal-error-chain.checklist.md`, \
         `foreign-error-types.checklist.md`, and \
         `foreign-error-attenuation.checklist.md`. `Box<dyn Error>` and \
         `Result<_, String>` from `antipatterns.checklist.md` roll into this \
         area's open count. Unused `_arg`, `&'static` fields, unnamed contract \
         bounds, and version-in-member are the Antipatterns area \
         (`version-in-member.checklist.md` is the version subset).\n"
    )?;

    writeln!(out, "## Resolution order\n")?;
    writeln!(
        out,
        "| Priority | Area | Open items | Checklist | Summary |"
    )?;
    writeln!(out, "| ---: | --- | ---: | --- | --- |")?;
    for area in report.areas() {
        let count = display_count(area, coverage);
        let checklist = checklist_link(area.checklist(), coverage);
        writeln!(
            out,
            "| {} | {} | {} | {} | [`{}`]({}) |",
            area.priority(),
            area.title(),
            count,
            checklist,
            area.summary(),
            area.summary(),
        )?;
    }
    writeln!(out)?;

    for area in report.areas() {
        writeln!(out, "## {}. {}\n", area.priority(), area.title())?;
        writeln!(out, "**Open items:** {}\n", display_count(area, coverage))?;
        if let Some(status) = coverage.and_then(|coverage| coverage.area(area.title())) {
            writeln!(out, "_{}._\n", status.status())?;
        }
        if coverage
            .and_then(|coverage| coverage.area(area.title()))
            .is_none_or(|status| status.ran())
        {
            writeln!(out, "_{}_\n", area.detail())?;
        }
        writeln!(
            out,
            "- Checklist: {}\n- Summary: [`{}`]({})\n",
            checklist_link(area.checklist(), coverage),
            area.summary(),
            area.summary(),
        )?;
    }

    Ok(out)
}

/// Render quality workspace summary markdown.
#[instrument(level = "debug", skip(report), err(level = "warn"))]
pub fn render_quality_workspace_summary_markdown(report: &QualityReport) -> CordialResult<String> {
    render_quality_workspace_summary_with_coverage(report, None)
}

#[instrument(level = "debug", skip(report, coverage))]
pub(super) fn render_quality_workspace_summary_with_coverage(
    report: &QualityReport,
    coverage: Option<&ReportRunCoverage>,
) -> CordialResult<String> {
    let mut out = String::new();
    writeln!(out, "# Quality workspace summary")?;
    writeln!(
        out,
        "\n**Total open items:** {}\n",
        report.total_open_items()
    )?;
    if coverage.is_some() {
        writeln!(
            out,
            "_Total includes only checks run now; skipped areas retain their last full-run count below._\n"
        )?;
    }
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
    for area in report.areas() {
        let status = coverage.and_then(|coverage| coverage.area(area.title()));
        writeln!(
            out,
            "| {} | {} | {} | {} | [`{}`]({}) | {} |",
            area.priority(),
            area.title(),
            display_count(area, coverage),
            checklist_link(area.checklist(), coverage),
            area.summary(),
            area.summary(),
            status.map_or_else(|| area.detail().as_str(), |status| status.status().as_str()),
        )?;
    }
    writeln!(out)?;

    Ok(out)
}

#[instrument(level = "debug", skip(area, coverage))]
fn display_count(area: &QualityAreaSummary, coverage: Option<&ReportRunCoverage>) -> String {
    match coverage.and_then(|coverage| coverage.area(area.title())) {
        Some(status) if status.ran() => match status.previous_open() {
            Some(previous) => format!("{} now; {previous} at last full run", area.open_items()),
            None => format!("{} now; no prior full run", area.open_items()),
        },
        Some(status) => status.previous_open().map_or_else(
            || "not run; no prior full run".to_string(),
            |previous| format!("not run; {previous} at last full run"),
        ),
        None => area.open_items().to_string(),
    }
}

#[instrument(level = "debug", skip(coverage))]
fn checklist_link(name: &str, coverage: Option<&ReportRunCoverage>) -> String {
    let stale = coverage.is_some_and(|coverage| coverage.checklist_is_stale(name));
    if stale {
        format!("[`{name}`]({name}) (stale)")
    } else {
        format!("[`{name}`]({name})")
    }
}
