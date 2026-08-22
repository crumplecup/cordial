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

#[test]
fn cli_exceptions_backup_and_load_roundtrip() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_minimal_crate(fixture.path(), "pub fn ok() {}")?;

    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let slug = cordial::project_slug_from_path(fixture.path());
    let project_store = store_home.path().join(&slug);
    fs::create_dir_all(project_store.join("exceptions/panics"))
        .into_diagnostic()
        .wrap_err("exceptions dir")?;
    fs::write(
        project_store.join("exceptions/panics/demo.json"),
        r#"[{"file":"src/lib.rs","reason":"intentional"}]"#,
    )
    .into_diagnostic()
    .wrap_err("write exception")?;
    fs::create_dir_all(project_store.join("patches"))
        .into_diagnostic()
        .wrap_err("patches dir")?;
    fs::write(
        project_store.join("patches/chrono.json"),
        r#"[{"path":"chrono::DateTime","reason":"skip"}]"#,
    )
    .into_diagnostic()
    .wrap_err("write coverage patch")?;

    let backup = cordial_command()
        .args([
            "--project",
            utf8_path(fixture.path())?,
            "--store-home",
            utf8_path(store_home.path())?,
            "exceptions",
            "backup",
        ])
        .output()
        .into_diagnostic()
        .wrap_err("exceptions backup")?;
    assert!(
        backup.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&backup.stderr)
    );
    let backup_body = String::from_utf8_lossy(&backup.stdout);
    assert!(backup_body.contains("backed up 2 exception files"));
    assert!(
        fixture
            .path()
            .join(".cordial-exceptions")
            .join(&slug)
            .join("exceptions/panics/demo.json")
            .is_file()
    );

    fs::remove_dir_all(project_store.join("exceptions"))
        .into_diagnostic()
        .wrap_err("wipe exceptions")?;
    fs::remove_dir_all(project_store.join("patches"))
        .into_diagnostic()
        .wrap_err("wipe patches")?;

    let elsewhere = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("cwd tempdir")?;
    let load = cordial_command()
        .current_dir(elsewhere.path())
        .args([
            "--project",
            utf8_path(fixture.path())?,
            "--store-home",
            utf8_path(store_home.path())?,
            "exceptions",
            "load",
            ".cordial-exceptions",
        ])
        .output()
        .into_diagnostic()
        .wrap_err("exceptions load")?;
    assert!(
        load.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&load.stderr)
    );
    let load_body = String::from_utf8_lossy(&load.stdout);
    assert!(load_body.contains("loaded 2 exception files"));
    assert!(project_store.join("exceptions/panics/demo.json").is_file());
    assert!(project_store.join("patches/chrono.json").is_file());

    let listed = cordial_command()
        .args([
            "--project",
            utf8_path(fixture.path())?,
            "--store-home",
            utf8_path(store_home.path())?,
            "exceptions",
            "list",
        ])
        .output()
        .into_diagnostic()
        .wrap_err("exceptions list")?;
    assert!(listed.status.success());
    let listed_body = String::from_utf8_lossy(&listed.stdout);
    assert!(listed_body.contains("exceptions/panics/demo.json"));
    assert!(listed_body.contains("patches/chrono.json"));
    Ok(())
}

#[test]
fn cli_exceptions_add_writes_quality_and_coverage_rows() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_minimal_crate(fixture.path(), "pub fn ok() {}")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let slug = cordial::project_slug_from_path(fixture.path());
    let project_store = store_home.path().join(&slug);

    let add = cordial_command()
        .args([
            "--project",
            utf8_path(fixture.path())?,
            "--store-home",
            utf8_path(store_home.path())?,
            "--crate-name",
            "demo",
            "exceptions",
            "add",
            "panics",
            "--file",
            "src/lib.rs",
            "--rule-id",
            "PANIC-SOURCE-PANIC",
            "--reason",
            "intentional",
        ])
        .output()
        .into_diagnostic()
        .wrap_err("exceptions add quality")?;
    assert!(
        add.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let add_body = String::from_utf8_lossy(&add.stdout);
    assert!(add_body.contains("added"));
    let quality = project_store.join("exceptions/panics/demo.json");
    assert!(quality.is_file());
    let quality_body = fs::read_to_string(&quality)
        .into_diagnostic()
        .wrap_err("read quality")?;
    assert!(quality_body.contains("PANIC-SOURCE-PANIC"));
    assert!(quality_body.contains("intentional"));

    let skip = cordial_command()
        .args([
            "--project",
            utf8_path(fixture.path())?,
            "--store-home",
            utf8_path(store_home.path())?,
            "exceptions",
            "add",
            "--patch-set",
            "chrono",
            "--path",
            "chrono::DateTime",
            "--reason",
            "upstream skip",
        ])
        .output()
        .into_diagnostic()
        .wrap_err("exceptions add coverage")?;
    assert!(
        skip.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&skip.stderr)
    );
    let skip_body = fs::read_to_string(project_store.join("patches/chrono.json"))
        .into_diagnostic()
        .wrap_err("read coverage")?;
    assert!(skip_body.contains("chrono::DateTime"));
    assert!(skip_body.contains("upstream skip"));
    Ok(())
}
