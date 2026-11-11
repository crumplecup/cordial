//! `cordial quality` from the command line: reports, the rollup, and the deny-open gate.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/cli_command.rs"]
mod cli_command;

use cli_command::{cordial_command, utf8_path};

#[path = "support/cli_crate.rs"]
mod cli_crate;

use cli_crate::write_minimal_crate;

#[cfg(feature = "hotpath")]
#[test]
fn cli_hotpath_uses_project_config_and_store_path() -> miette::Result<()> {
    cordial::init_tracing();
    let project = tempfile::tempdir().into_diagnostic().wrap_err("project")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store home")?;
    fs::write(
        project.path().join("cordial.toml"),
        "[hotpath]\noutput_path = \"profile.json\"\n",
    )
    .into_diagnostic()
    .wrap_err("write config")?;

    let output = cordial_command()
        .args([
            "--project",
            utf8_path(project.path())?,
            "--store-home",
            utf8_path(store_home.path())?,
            "instructions",
        ])
        .output()
        .into_diagnostic()
        .wrap_err("run cordial instructions")?;
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let slug = cordial::project_slug_from_path(project.path());
    assert!(store_home.path().join(slug).join("profile.json").is_file());
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
