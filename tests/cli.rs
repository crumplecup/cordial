use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::process::Command;

fn cordial_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cordial"));
    command.env("RUST_LOG", "info");
    command
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
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_minimal_crate(
        fixture.path(),
        include_str!("fixtures/panics/cli_quality.rs"),
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
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Starting: "), "stderr: {stderr}");
    assert!(stderr.contains("Finished: "), "stderr: {stderr}");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Starting: "));
    let slug = cordial::project_slug_from_path(fixture.path());
    let project_store = store.path().join(&slug);
    assert!(project_store.join("findings/panics.csv").is_file());
    assert!(project_store.join("findings/rollup-summary.md").is_file());
    assert!(project_store.join("findings/quality-report.md").is_file());
    assert!(project_store.join("findings/summary.md").is_file());
    let layout = cordial::StoreLayout::from_root(&project_store, slug);
    let list = cordial::Worklist::read(&layout)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("quality did not write a worklist"))?;
    assert!(list.open_count() > 0);
    assert_eq!(*list.record().generation(), 1);
    let panic_items: Vec<_> = list
        .items()
        .iter()
        .filter(|item| item.checklist() == "panics.checklist.md")
        .collect();
    assert!(!panic_items.is_empty());
    assert!(
        panic_items.iter().all(|item| item.rule_id().is_some()),
        "panic checklist items should retain finding rule IDs"
    );
    Ok(())
}

#[test]
fn cli_quality_deny_open_fails_after_writing_reports() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_minimal_crate(
        fixture.path(),
        include_str!("fixtures/panics/cli_quality.rs"),
    )?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let output = cordial_command()
        .env("CORDIAL_NO_PROGRESS", "1")
        .args([
            "--project",
            utf8_path(fixture.path())?,
            "--store-home",
            utf8_path(store.path())?,
            "quality",
            "--deny-open",
        ])
        .output()
        .into_diagnostic()
        .wrap_err("run cordial quality --deny-open")?;

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("open action items denied"),
        "stderr: {stderr}"
    );
    assert!(!stderr.contains("Starting: "), "stderr: {stderr}");
    let slug = cordial::project_slug_from_path(fixture.path());
    let project_store = store.path().join(&slug);
    assert!(project_store.join("findings/quality-report.md").is_file());
    assert!(project_store.join("findings/rollup-summary.md").is_file());
    Ok(())
}

#[test]
fn cli_quality_deny_open_allows_inventory_only_findings() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_minimal_crate(
        fixture.path(),
        r#"
pub fn read_text(path: &std::path::Path) -> std::io::Result<String> {
    let text = std::fs::read_to_string(path)?;
    Ok(text)
}
"#,
    )?;
    fs::write(
        fixture.path().join("cordial.toml"),
        r#"
[panics]
enabled = false

[tracing]
enabled = false

[allows]
enabled = false

[modularity]
enabled = false

[derives]
enabled = false

[error_chain]
enabled = false

[internal_error_chain]
enabled = false

[foreign_error_types]
enabled = false

[foreign_error_attenuation]
enabled = false

[antipatterns]
enabled = false

[cfg_scatter]
enabled = false

[cfg_hygiene]
enabled = false

[visibility]
enabled = false

[cli_layout]
enabled = false

[crate_attrs]
enabled = false

[doc_warnings]
enabled = false

[glob_imports]
enabled = false

[inline_tests]
enabled = false

[verus_warnings]
enabled = false

[creusot_diagnostics]
enabled = false

[dependency_freshness]
enabled = false

[proof_patterns]
enabled = false

[pageantry]
enabled = false
"#,
    )
    .into_diagnostic()
    .wrap_err("cordial.toml")?;

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
            "--deny-open",
        ])
        .output()
        .into_diagnostic()
        .wrap_err("run cordial quality --deny-open")?;

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let slug = cordial::project_slug_from_path(fixture.path());
    let project_store = store.path().join(&slug);
    assert!(project_store.join("findings/error-sites.csv").is_file());
    let quality_report = fs::read_to_string(project_store.join("findings/quality-report.md"))
        .into_diagnostic()
        .wrap_err("quality report")?;
    assert!(quality_report.contains("**Total open items:** 0"));
    Ok(())
}

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

#[test]
fn cli_explain_lists_compiled_etiquettes() -> miette::Result<()> {
    cordial::init_tracing();
    let output = cordial_command()
        .arg("explain")
        .output()
        .into_diagnostic()
        .wrap_err("cordial explain")?;
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body = String::from_utf8_lossy(&output.stdout);
    assert!(body.contains("doc_warnings"));
    assert!(body.contains("panics"));
    Ok(())
}

#[test]
fn cli_tiers_and_explain_use_project_assignment() -> miette::Result<()> {
    cordial::init_tracing();
    let project = tempfile::tempdir().into_diagnostic().wrap_err("project")?;
    fs::write(
        project.path().join("cordial.toml"),
        "[tiers.assign]\ndoc_warnings = \"deep\"\n",
    )
    .into_diagnostic()
    .wrap_err("tier config")?;
    let project_path = utf8_path(project.path())?;
    let run = |args: &[&str]| -> miette::Result<String> {
        let output = cordial_command()
            .args(["--project", project_path])
            .args(args)
            .output()
            .into_diagnostic()
            .wrap_err("run cordial")?;
        assert!(
            output.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    };
    let tiers = run(&["tiers"])?;
    assert!(tiers.contains("Default tiers: quick, build"));
    assert!(tiers.contains("deep:\n"));
    assert!(tiers.contains("doc_warnings (config)"));
    let list = run(&["explain"])?;
    assert!(
        list.lines()
            .any(|line| line.contains("doc_warnings") && line.contains("[deep]"))
    );
    let page = run(&["explain", "DOC-WARNING-001"])?;
    assert!(page.contains("**Tier:** `deep` (config)"));
    Ok(())
}

#[test]
fn cli_submit_carries_unverified_claim_and_repeats_tier() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("project")?;
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    write_minimal_crate(
        fixture.path(),
        include_str!("fixtures/panics/cli_quality.rs"),
    )?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tiers.assign]\nfuture_etiquette = \"empty\"\n",
    )
    .into_diagnostic()
    .wrap_err("tier config")?;
    let project = utf8_path(fixture.path())?;
    let home = utf8_path(store.path())?;
    let run = |args: &[&str]| -> miette::Result<String> {
        let output = cordial_command()
            .env("CORDIAL_NO_PROGRESS", "1")
            .args(["--project", project, "--store-home", home])
            .args(args)
            .output()
            .into_diagnostic()
            .wrap_err("run cordial")?;
        assert!(
            output.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    };
    run(&["quality", "--etiquette", "panics"])?;
    let layout = cordial::StoreLayout::from_root(
        store
            .path()
            .join(cordial::project_slug_from_path(fixture.path())),
        cordial::project_slug_from_path(fixture.path()),
    );
    let first = cordial::Worklist::read(&layout)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing list"))?;
    let claim = first
        .items()
        .iter()
        .find(|item| *item.state() == cordial::ItemState::Open)
        .ok_or_else(|| miette::miette!("missing open item"))?
        .id()
        .as_str()
        .to_string();
    run(&["done", &claim])?;
    let skipped = run(&["submit", "--tier", "empty"])?;
    assert!(skipped.contains("Unverified from tiers not run:"));
    assert!(!skipped.contains("Cleared claims:"));
    let carried = cordial::Worklist::read(&layout)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing carried list"))?;
    assert_eq!(carried.record().tiers(), &["empty"]);
    let item = carried
        .items()
        .iter()
        .find(|item| item.id().as_str() == claim)
        .ok_or_else(|| miette::miette!("claim disappeared"))?;
    assert!(*item.unverified());
    assert_eq!(*item.state(), cordial::ItemState::Done);
    run(&["submit"])?;
    let repeated = cordial::Worklist::read(&layout)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing repeated list"))?;
    assert_eq!(repeated.record().tiers(), &["empty"]);
    assert!(
        repeated
            .items()
            .iter()
            .any(|item| item.id().as_str() == claim && *item.unverified())
    );
    let verified = run(&["submit", "--tier", "quick"])?;
    assert!(verified.contains("Reopened claims:"));
    let fresh = cordial::Worklist::read(&layout)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing verified list"))?;
    assert!(
        fresh
            .items()
            .iter()
            .any(|item| item.id().as_str() == claim && !*item.unverified())
    );
    Ok(())
}

#[test]
fn cli_worklist_json_round_trip_and_errors() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("project")?;
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    write_minimal_crate(
        fixture.path(),
        include_str!("fixtures/panics/cli_quality.rs"),
    )?;
    let project = utf8_path(fixture.path())?;
    let home = utf8_path(store.path())?;
    let run = |args: &[&str]| -> miette::Result<std::process::Output> {
        cordial_command()
            .args(["--project", project, "--store-home", home])
            .args(args)
            .output()
            .into_diagnostic()
            .wrap_err("run cordial")
    };
    let read = |output: &std::process::Output| -> miette::Result<serde_json::Value> {
        serde_json::from_slice(&output.stdout)
            .into_diagnostic()
            .wrap_err(format!(
                "JSON stdout: {}",
                String::from_utf8_lossy(&output.stdout)
            ))
    };
    let initial = run(&["quality", "--etiquette", "panics"])?;
    assert!(
        initial.status.success(),
        "{}",
        String::from_utf8_lossy(&initial.stderr)
    );
    let open_output = run(&["open", "--json"])?;
    assert!(
        open_output.status.success(),
        "{}",
        String::from_utf8_lossy(&open_output.stderr)
    );
    let open = read(&open_output)?;
    assert_eq!(open["version"], 1);
    assert_eq!(open["command"], "open");
    assert_eq!(open["list_generation"], 1);
    assert!(open["store_path"].as_str().is_some());
    assert!(open["current"]["rule_id"].as_str().is_some());
    assert!(!String::from_utf8_lossy(&open_output.stdout).contains("Starting:"));
    let id = open["current"]["id"]
        .as_str()
        .ok_or_else(|| miette::miette!("no id"))?;

    let peek = read(&run(&["peek", "--json"])?)?;
    assert_eq!(peek["current"]["id"], id);
    let stats = read(&run(&["stats", "--verbose", "--json"])?)?;
    assert_eq!(stats["items"][0]["id"], id);

    let bad = run(&["done", id, "aaaaaa", "--json"])?;
    assert!(!bad.status.success());
    let error = read(&bad)?;
    assert_eq!(error["error"]["kind"], "UnknownId");
    assert_eq!(read(&run(&["peek", "--json"])?)?["current"]["id"], id);

    let done = read(&run(&["done", id, "--json"])?)?;
    assert_eq!(done["checked_off"][0]["was"], "open");
    assert_eq!(done["checked_off"][0]["now"], "done");
    let again = read(&run(&["done", id, "--json"])?)?;
    assert_eq!(again["checked_off"][0]["was"], "done");

    fs::write(
        fixture.path().join("src/lib.rs"),
        "pub fn boom() {}\npub fn quiet() {}\n",
    )
    .into_diagnostic()
    .wrap_err("fix fixture")?;
    let submit_output = run(&["submit", "--tier", "quick", "--json"])?;
    assert!(
        submit_output.status.success(),
        "{}",
        String::from_utf8_lossy(&submit_output.stderr)
    );
    let submit = read(&submit_output)?;
    assert_eq!(submit["command"], "submit");
    assert_eq!(submit["previous_generation"], 1);
    assert_eq!(submit["list_generation"], 2);
    assert!(submit["outcomes"].as_array().is_some_and(|outcomes| {
        outcomes
            .iter()
            .any(|outcome| outcome["id"] == id && outcome["outcome"] == "cleared")
    }));
    Ok(())
}

#[test]
fn cli_worklist_json_skip_and_denied_submit() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("project")?;
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    write_minimal_crate(
        fixture.path(),
        include_str!("fixtures/panics/cli_quality.rs"),
    )?;
    let project = utf8_path(fixture.path())?;
    let home = utf8_path(store.path())?;
    let run = |args: &[&str]| -> miette::Result<std::process::Output> {
        cordial_command()
            .args(["--project", project, "--store-home", home])
            .args(args)
            .output()
            .into_diagnostic()
            .wrap_err("run cordial")
    };
    assert!(run(&["quality", "--etiquette", "panics"])?.status.success());
    let open: serde_json::Value =
        serde_json::from_slice(&run(&["open", "--json"])?.stdout).into_diagnostic()?;
    let id = open["current"]["id"]
        .as_str()
        .ok_or_else(|| miette::miette!("no id"))?;
    let skipped = run(&["skip", id, "--reason", "needs a design decision", "--json"])?;
    assert!(
        skipped.status.success(),
        "{}",
        String::from_utf8_lossy(&skipped.stderr)
    );
    let skipped: serde_json::Value = serde_json::from_slice(&skipped.stdout).into_diagnostic()?;
    assert_eq!(skipped["checked_off"][0]["now"], "skipped");
    let stats: serde_json::Value =
        serde_json::from_slice(&run(&["stats", "--verbose", "--json"])?.stdout)
            .into_diagnostic()?;
    assert_eq!(
        stats["items"][0]["skip_reasons"][0],
        "needs a design decision"
    );

    let denied = run(&["submit", "--tier", "quick", "--deny-open", "--json"])?;
    assert!(!denied.status.success());
    let response: serde_json::Value = serde_json::from_slice(&denied.stdout).into_diagnostic()?;
    assert_eq!(response["command"], "submit");
    assert_eq!(response["error"]["kind"], "CommandFailed");
    assert_eq!(response["list_generation"], 2);
    Ok(())
}

#[test]
fn cli_explain_rule_id_prints_page() -> miette::Result<()> {
    cordial::init_tracing();
    let output = cordial_command()
        .args(["explain", "DOC-WARNING-001"])
        .output()
        .into_diagnostic()
        .wrap_err("cordial explain DOC-WARNING-001")?;
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body = String::from_utf8_lossy(&output.stdout);
    assert!(body.contains("`doc_warnings`"));
    assert!(body.contains("## Why"));
    assert!(body.contains("## Opt out"));
    Ok(())
}

#[test]
fn cli_explain_unknown_id_fails() -> miette::Result<()> {
    cordial::init_tracing();
    let output = cordial_command()
        .args(["explain", "not-a-real-lint"])
        .output()
        .into_diagnostic()
        .wrap_err("cordial explain unknown")?;
    assert!(!output.status.success());
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(
        err.contains("not-a-real-lint") || err.contains("etiquette not registered"),
        "stderr: {err}"
    );
    Ok(())
}
