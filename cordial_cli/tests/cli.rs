use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::process::Command;

fn cordial_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cordial"))
}

fn utf8_path(path: &std::path::Path) -> miette::Result<&str> {
    path.to_str()
        .ok_or_else(|| miette::miette!("path is not UTF-8: {}", path.display()))
}

fn write_minimal_crate(root: &std::path::Path, lib_rs: &str) -> miette::Result<()> {
    fs::create_dir_all(root.join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(root.join("src/lib.rs"), lib_rs)
        .into_diagnostic()
        .wrap_err("write fixture")?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .into_diagnostic()
    .wrap_err("write manifest")?;
    Ok(())
}

#[test]
fn cli_quality_writes_reports_and_rollup() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_minimal_crate(
        fixture.path(),
        "pub fn boom() { panic!(\"x\"); }\n\npub fn quiet() {}",
    )?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let output = cordial_command()
        .args([
            "--project",
            utf8_path(fixture.path())?,
            "--store-home",
            utf8_path(store.path())?,
            "quality",
        ])
        .output()
        .into_diagnostic()
        .wrap_err("run cordial quality")?;

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let slug = cordial::project_slug_from_path(fixture.path());
    let project_store = store.path().join(&slug);
    assert!(project_store.join("findings/panics.csv").is_file());
    assert!(project_store.join("findings/rollup-summary.md").is_file());
    assert!(project_store.join("findings/quality-report.md").is_file());
    assert!(project_store.join("findings/summary.md").is_file());
    Ok(())
}

#[test]
fn cli_view_prints_artifact() -> miette::Result<()> {
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
