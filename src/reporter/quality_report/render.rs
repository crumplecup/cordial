//! Markdown rendering for the unified quality report.

use std::fmt::Write as _;

use crate::error::CordialResult;

use super::QualityReport;

use tracing::instrument;

/// Render quality report markdown.
#[instrument(level = "debug", skip(report), err(level = "warn"))]
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

/// Render quality workspace summary markdown.
#[instrument(level = "debug", skip(report), err(level = "warn"))]
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
