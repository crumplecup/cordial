//! `cordial exceptions`: backing up and loading the registry, adding rows, editing and removing one.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/cli_command.rs"]
mod cli_command;

use cli_command::{cordial_command, utf8_path};

#[path = "support/cli_crate.rs"]
mod cli_crate;

use cli_crate::write_minimal_crate;

#[test]
fn cli_exceptions_backup_and_load_roundtrip() -> miette::Result<()> {
    cordial::init_tracing();
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
    let backup_body = String::from_utf8_lossy(&backup.stderr);
    assert!(
        backup_body.contains("backed up exception files"),
        "stderr: {backup_body}"
    );
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
    let load_body = String::from_utf8_lossy(&load.stderr);
    assert!(
        load_body.contains("loaded exception files"),
        "stderr: {load_body}"
    );
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
    cordial::init_tracing();
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
    let add_body = String::from_utf8_lossy(&add.stderr);
    assert!(add_body.contains("added"), "stderr: {add_body}");
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

#[test]
fn cli_exceptions_edit_and_remove_change_one_row() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_minimal_crate(fixture.path(), "pub fn ok() {}")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let slug = cordial::project_slug_from_path(fixture.path());
    let quality = store_home
        .path()
        .join(&slug)
        .join("exceptions/panics/demo.json");

    let run = |extra: &[&str]| -> miette::Result<std::process::Output> {
        let mut args = vec![
            "--project",
            utf8_path(fixture.path())?,
            "--store-home",
            utf8_path(store_home.path())?,
            "--crate-name",
            "demo",
            "exceptions",
        ];
        args.extend_from_slice(extra);
        cordial_command()
            .args(args)
            .output()
            .into_diagnostic()
            .wrap_err("cordial exceptions")
    };

    for (context, reason) in [("ext::a", "first"), ("ext::b", "second")] {
        let added = run(&[
            "add",
            "panics",
            "--file",
            "src/lib.rs",
            "--rule-id",
            "PANIC-SOURCE-PANIC",
            "--context",
            context,
            "--reason",
            reason,
        ])?;
        assert!(added.status.success());
    }

    let ambiguous = run(&["remove", "panics", "--rule-id", "PANIC-SOURCE-PANIC"])?;
    assert!(!ambiguous.status.success());
    assert!(String::from_utf8_lossy(&ambiguous.stderr).contains("2 exceptions"));

    let edited = run(&[
        "edit",
        "panics",
        "--context",
        "ext::a",
        "--new-context",
        "a",
    ])?;
    assert!(
        edited.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&edited.stderr)
    );
    let body = fs::read_to_string(&quality)
        .into_diagnostic()
        .wrap_err("read")?;
    assert!(body.contains("\"context\": \"a\""));
    assert!(!body.contains("ext::a"));

    let removed = run(&["remove", "panics", "--context", "ext::b"])?;
    assert!(removed.status.success());
    let body = fs::read_to_string(&quality)
        .into_diagnostic()
        .wrap_err("read")?;
    assert!(!body.contains("second"));
    assert!(body.contains("first"));

    let last = run(&["remove", "panics", "--context", "a"])?;
    assert!(last.status.success());
    assert!(!quality.exists());
    Ok(())
}
