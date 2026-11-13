//! A filter that names test binaries builds only those: nextest applies a filterset
//! after building, so without `--test` every binary in the repository compiles.

use cordial::perf::build_targets;
use miette::{IntoDiagnostic, Result};

fn project(tests: &[&str]) -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("tests")).into_diagnostic()?;
    for name in tests {
        std::fs::write(root.path().join("tests").join(format!("{name}.rs")), "")
            .into_diagnostic()?;
    }
    Ok(root)
}

#[test]
fn a_filter_of_binaries_builds_only_those_binaries() -> Result<()> {
    cordial::init_tracing();
    let root = project(&["context_crates", "verus_ir", "panics_verus"])?;
    assert_eq!(
        build_targets(root.path(), "binary(context_crates)"),
        Some(vec!["context_crates".to_owned()])
    );
    assert_eq!(
        build_targets(root.path(), "binary(verus_ir) | binary(panics_verus)"),
        Some(vec!["verus_ir".to_owned(), "panics_verus".to_owned()])
    );
    Ok(())
}

#[test]
fn anything_more_than_binaries_cannot_be_scoped() -> Result<()> {
    cordial::init_tracing();
    let root = project(&["context_crates"])?;
    for filter in [
        "all()",
        "binary(context_crates) & test(an_edit)",
        "test(an_edit)",
        "binary(context_crates) | test(x)",
        "binary(not_a_test_file)",
        "binary()",
        "binary(a b)",
    ] {
        assert_eq!(build_targets(root.path(), filter), None, "{filter}");
    }
    Ok(())
}
