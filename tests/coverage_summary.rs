#![cfg(any(
    feature = "homecoming_std",
    feature = "amenable_std",
    feature = "elicitation"
))]

use cordial::{CoveragePluginSummary, CoverageSummary, render_coverage_summary_markdown};
use miette::IntoDiagnostic;

#[test]
fn render_empty_coverage_summary_notes_no_plugins() -> miette::Result<()> {
    let body = render_coverage_summary_markdown(&CoverageSummary {
        plugins: vec![],
        extra_artifacts: Vec::new(),
    })
    .into_diagnostic()?;
    assert!(body.contains("# Coverage summary"));
    assert!(body.contains("No coverage plugins ran"));
    Ok(())
}

#[test]
fn render_coverage_summary_includes_plugin_sections() -> miette::Result<()> {
    let summary = CoverageSummary {
        plugins: vec![
            CoveragePluginSummary {
                plugin_id: "homecoming-std-coverage".to_string(),
                plugin_name: "Homecoming std coverage".to_string(),
                body: "# Framework trait coverage summary\n\n**Complete:** 1\n".to_string(),
            },
            CoveragePluginSummary {
                plugin_id: "elicitation-coverage".to_string(),
                plugin_name: "Elicitation coverage".to_string(),
                body: "### Impl coverage\n\n| Crate | Types |\n".to_string(),
            },
        ],
        extra_artifacts: Vec::new(),
    };
    let body = render_coverage_summary_markdown(&summary).into_diagnostic()?;
    assert!(body.contains("Rollup for **2** registered coverage plugin"));
    assert!(body.contains("## Homecoming std coverage"));
    assert!(body.contains("## Elicitation coverage"));
    assert!(body.contains("---"));
    Ok(())
}
