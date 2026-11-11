//! `cordial explain` and tier assignment: compiled etiquettes, rule pages, and unknown ids.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/cli_command.rs"]
mod cli_command;

use cli_command::{cordial_command, utf8_path};

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
    assert!(tiers.contains("Default tiers: build"));
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
