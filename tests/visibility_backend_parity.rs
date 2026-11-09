//! Module hierarchy evidence, graph-only visibility policy and revision recovery.
#![cfg(feature = "visibility")]
use miette::{IntoDiagnostic, Result};

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn cyclic_module_paths_fail_instead_of_recursing_without_bound() -> Result<()> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("src/lib.rs"),
        "#[path = \"lib.rs\"] pub mod again;\n",
    )
    .into_diagnostic()?;
    assert!(
        cordial::scan_crate_visibility(root.path(), cordial::VisibilityThresholds::default())
            .is_err()
    );
    Ok(())
}
