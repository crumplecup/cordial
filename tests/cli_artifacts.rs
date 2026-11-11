//! `cordial view` and `cordial export` read the artifacts and cached IR a run left behind.

use miette::{IntoDiagnostic, WrapErr};

#[path = "support/cli_command.rs"]
mod cli_command;

use cli_command::{cordial_command, utf8_path};

#[path = "support/cli_crate.rs"]
mod cli_crate;

use cli_crate::write_minimal_crate;

#[test]
fn cli_view_prints_artifact() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_minimal_crate(fixture.path(), "pub fn ok() {}")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    cordial_command()
        .args([
            "--project",
            utf8_path(fixture.path())?,
            "--store-home",
            utf8_path(store.path())?,
            "quality",
        ])
        .output()
        .into_diagnostic()
        .wrap_err("seed reports")?;

    let output = cordial_command()
        .args([
            "--project",
            utf8_path(fixture.path())?,
            "--store-home",
            utf8_path(store.path())?,
            "view",
            "findings/rollup-summary.md",
        ])
        .output()
        .into_diagnostic()
        .wrap_err("view rollup")?;

    assert!(output.status.success());
    let body = String::from_utf8_lossy(&output.stdout);
    assert!(body.contains("# Cordial rollup summary"));
    Ok(())
}

#[test]
fn cli_export_surreal_reads_cached_ir() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_minimal_crate(fixture.path(), "pub fn ok() {}")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    cordial_command()
        .args([
            "--project",
            utf8_path(fixture.path())?,
            "--store-home",
            utf8_path(store.path())?,
            "quality",
        ])
        .output()
        .into_diagnostic()
        .wrap_err("seed cache")?;

    let output = cordial_command()
        .args([
            "--project",
            utf8_path(fixture.path())?,
            "--store-home",
            utf8_path(store.path())?,
            "--crate-name",
            "demo",
            "export",
            "surreal",
        ])
        .output()
        .into_diagnostic()
        .wrap_err("export surreal")?;

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body = String::from_utf8_lossy(&output.stdout);
    assert!(body.contains("\"nodes\""));
    assert!(body.contains("\"edges\""));
    Ok(())
}
