#![cfg(feature = "quality")]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use cordial::{
    GateLedger, SessionBuilder, StoreLayout, TierSelection, all_plugins, assigned_tier,
    etiquettes_from_plugins, load_cordial_config, project_slug_from_path,
};
use miette::{IntoDiagnostic, WrapErr};

fn project() -> miette::Result<tempfile::TempDir> {
    let project = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(project.path().join("src")).into_diagnostic()?;
    fs::write(
        project.path().join("Cargo.toml"),
        "[package]\nname = \"tier-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        project.path().join("src/lib.rs"),
        include_str!("fixtures/panics/cli_quality.rs"),
    )
    .into_diagnostic()?;
    Ok(project)
}

fn run(project: &Path, home: &Path, args: &[&str]) -> miette::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_cordial"))
        .arg("--project")
        .arg(project)
        .arg("--store-home")
        .arg(home)
        .args(args)
        .env("CORDIAL_NO_PROGRESS", "1")
        .output()
        .into_diagnostic()
        .wrap_err_with(|| format!("cordial {}", args.join(" ")))
}

fn store(project: &Path, home: &Path) -> StoreLayout {
    let slug = project_slug_from_path(project);
    StoreLayout::from_root(home.join(&slug), slug)
}

#[test]
fn resolves_defaults_unions_and_project_assignments() -> miette::Result<()> {
    cordial::init_tracing();
    let project = project()?;
    let home = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(project.path()).build();
    let etiquettes = etiquettes_from_plugins(&all_plugins(), &session);
    let config = load_cordial_config(project.path(), home.path());

    let defaults = TierSelection::resolve(&etiquettes, &config, None).into_diagnostic()?;
    let antipatterns = etiquettes
        .iter()
        .find(|etiquette| etiquette.id() == "antipatterns")
        .ok_or_else(|| miette::miette!("antipatterns unavailable"))?;
    let freshness = etiquettes
        .iter()
        .find(|etiquette| etiquette.id() == "dependency_freshness")
        .ok_or_else(|| miette::miette!("dependency freshness unavailable"))?;
    assert!(defaults.includes(antipatterns.as_ref(), &config));
    assert!(!defaults.includes(freshness.as_ref(), &config));

    let union = ["quick".to_string(), "deep".to_string()];
    let selected = TierSelection::resolve(&etiquettes, &config, Some(&union)).into_diagnostic()?;
    assert!(!selected.includes(antipatterns.as_ref(), &config));
    assert!(selected.includes(freshness.as_ref(), &config));

    fs::write(
        project.path().join("cordial.toml"),
        "[tiers]\ndefault = [\"ci\"]\n[tiers.assign]\nantipatterns = \"ci\"\n",
    )
    .into_diagnostic()?;
    let config = load_cordial_config(project.path(), home.path());
    let selected = TierSelection::resolve(&etiquettes, &config, None).into_diagnostic()?;
    assert_eq!(assigned_tier(antipatterns.as_ref(), &config).as_str(), "ci");
    assert!(selected.includes(antipatterns.as_ref(), &config));
    assert!(!selected.includes(freshness.as_ref(), &config));

    let unknown = ["missing".to_string()];
    let error = TierSelection::resolve(&etiquettes, &config, Some(&unknown)).unwrap_err();
    assert!(error.to_string().contains("known tiers"));
    assert!(error.to_string().contains("ci"));
    Ok(())
}

#[test]
fn named_etiquette_overrides_tier_but_disabled_still_wins() -> miette::Result<()> {
    cordial::init_tracing();
    let project = project()?;
    let home = tempfile::tempdir().into_diagnostic()?;
    let output = run(
        project.path(),
        home.path(),
        &["quality", "--tier", "deep", "--etiquette", "panics"],
    )?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ledger = GateLedger::read(&store(project.path(), home.path())).into_diagnostic()?;
    assert!(ledger.entries().contains_key("panics"));
    assert_eq!(
        ledger
            .entries()
            .get("panics")
            .map(|entry| entry.tier().as_str()),
        Some("quick")
    );

    fs::write(
        project.path().join("cordial.toml"),
        "[panics]\nenabled = false\n",
    )
    .into_diagnostic()?;
    let disabled_home = tempfile::tempdir().into_diagnostic()?;
    let output = run(
        project.path(),
        disabled_home.path(),
        &["quality", "--tier", "deep", "--etiquette", "panics"],
    )?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ledger =
        GateLedger::read(&store(project.path(), disabled_home.path())).into_diagnostic()?;
    assert!(!ledger.entries().contains_key("panics"));

    let invalid_home = tempfile::tempdir().into_diagnostic()?;
    let output = run(
        project.path(),
        invalid_home.path(),
        &["quality", "--tier", "absent"],
    )?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("known tiers"));
    assert!(
        !store(project.path(), invalid_home.path())
            .root()
            .join("ledger.json")
            .exists()
    );
    Ok(())
}
