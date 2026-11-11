//! `cordial submit` and the worklist JSON: unverified claims, round trips, skips, and denied submits.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/cli_command.rs"]
mod cli_command;

use cli_command::{cordial_command, utf8_path};

#[path = "support/cli_crate.rs"]
mod cli_crate;

use cli_crate::write_minimal_crate;

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
