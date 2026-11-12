//! A print in code only a feature's build activates is still a print: the
//! observability inventory is read again in each analysed cfg context.

use cordial::{IrView, TracingStdioPolicy, tracing_print_findings};
use miette::{IntoDiagnostic, Result};

#[path = "support/projected_crate.rs"]
mod projected_crate;

const LIBRARY: &str = include_str!("fixtures/tracing_print_contexts/lib.rs.txt");
const GATED: &str = include_str!("fixtures/tracing_print_contexts/gated.rs.txt");

#[test]
fn a_print_behind_a_feature_is_seen() -> Result<()> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='printy'\nversion='0.1.0'\nedition='2024'\n[features]\nextra=[]\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    std::fs::write(root.path().join("src/gated.rs"), GATED).into_diagnostic()?;
    let inventory = projected_crate::projected_crate(root.path())?
        .source_observability()
        .into_diagnostic()?;
    let found =
        tracing_print_findings(&inventory, &TracingStdioPolicy::default()).into_diagnostic()?;
    let files: Vec<_> = found.iter().map(|record| record.file().clone()).collect();
    assert_eq!(files.len(), 1, "{found:?}");
    assert!(files[0].ends_with("gated.rs"), "{files:?}");
    Ok(())
}
