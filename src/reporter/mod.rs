//! Built-in reporters and summary builders.
//!
//! Reporters convert post-assessment findings into local artifacts such as CSV,
//! Markdown checklists, rollups, quality summaries, and coverage summaries. The
//! session always adds the rollup reporter; feature-gated quality and coverage
//! summaries are added when the active plugin or etiquette set needs them.
//!
//! Custom etiquette authors usually implement [`Reporter`](crate::Reporter)
//! directly. The items re-exported here are the first-party report shapes used
//! by the CLI and built-in standards.

#[cfg(any(
    feature = "homecoming_std",
    feature = "amenable_std",
    feature = "elicitation"
))]
mod coverage_summary;
#[cfg(feature = "elicitation")]
mod elicitation_summary;
#[cfg(feature = "quality")]
mod quality_report;
mod rollup;

#[cfg(any(
    feature = "homecoming_std",
    feature = "amenable_std",
    feature = "elicitation"
))]
pub use coverage_summary::{
    CoveragePluginSummary, CoverageSummary, build_coverage_summary,
    render_coverage_summary_markdown,
};
#[cfg(feature = "quality")]
pub use quality_report::{
    QualityAreaSummary, QualityReport, QualityReportReporter, build_quality_report,
    render_quality_report_markdown, render_quality_workspace_summary_markdown,
};
pub use rollup::RollupReporter;
