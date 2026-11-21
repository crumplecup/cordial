//! A print in code only a feature's build activates is still a print: the
//! observability inventory is read again in each analysed cfg context.

use cordial::{IrView, TracingStdioPolicy, tracing_print_findings};
use miette::{IntoDiagnostic, Result};

#[path = "support/projected_crate.rs"]
mod projected_crate;

const LIBRARY: &str = include_str!("fixtures/tracing_print_contexts/lib.rs.txt");
const GATED: &str = include_str!("fixtures/tracing_print_contexts/gated.rs.txt");
const INLINE: &str = include_str!("fixtures/tracing_print_contexts/inline.rs.txt");

fn print_files(library: &str) -> Result<Vec<std::path::PathBuf>> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='printy'\nversion='0.1.0'\nedition='2024'\n[features]\nextra=[]\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), library).into_diagnostic()?;
    std::fs::write(root.path().join("src/gated.rs"), GATED).into_diagnostic()?;
    let inventory = projected_crate::projected_crate(root.path())?
        .source_observability()
        .into_diagnostic()?;
    let found =
        tracing_print_findings(&inventory, &TracingStdioPolicy::default()).into_diagnostic()?;
    Ok(found.iter().map(|record| record.file().clone()).collect())
}

#[test]
fn a_print_behind_a_feature_is_seen() -> Result<()> {
    let files = print_files(LIBRARY)?;
    assert_eq!(files.len(), 1, "{files:?}");
    assert!(files[0].ends_with("gated.rs"), "{files:?}");
    Ok(())
}

#[test]
fn a_feature_gated_inline_print_keeps_macro_identity() -> Result<()> {
    let files = print_files(INLINE)?;
    assert_eq!(files.len(), 1, "{files:?}");
    assert!(files[0].ends_with("lib.rs"), "{files:?}");
    Ok(())
}
