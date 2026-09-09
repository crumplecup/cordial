#![cfg(any(
    feature = "homecoming_std",
    feature = "amenable_std",
    feature = "elicitation"
))]

use cordial::{CoveragePluginSummary, CoverageSummary, render_coverage_summary_markdown};
use miette::IntoDiagnostic;

#[test]
fn render_empty_coverage_summary_notes_no_plugins() -> miette::Result<()> {
    cordial::init_tracing();
    let body = render_coverage_summary_markdown(&CoverageSummary::new(vec![], Vec::new()))
        .into_diagnostic()?;
    assert!(body.contains("# Coverage summary"));
    assert!(body.contains("No coverage plugins ran"));
    Ok(())
}

#[test]
fn render_coverage_summary_includes_plugin_sections() -> miette::Result<()> {
    cordial::init_tracing();
    let summary = CoverageSummary::new(
        vec![
            CoveragePluginSummary::new(
                "homecoming-std-coverage".to_string(),
                "Homecoming std coverage".to_string(),
                "# Framework trait coverage summary\n\n**Complete:** 1\n".to_string(),
            ),
            CoveragePluginSummary::new(
                "elicitation-coverage".to_string(),
                "Elicitation coverage".to_string(),
                "### Impl coverage\n\n| Crate | Types |\n".to_string(),
            ),
        ],
        Vec::new(),
    );
    let body = render_coverage_summary_markdown(&summary).into_diagnostic()?;
    assert!(body.contains("Rollup for **2** registered coverage plugin"));
    assert!(body.contains("## Homecoming std coverage"));
    assert!(body.contains("## Elicitation coverage"));
    assert!(body.contains("---"));
    Ok(())
}
