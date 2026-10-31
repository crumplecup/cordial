#![cfg(feature = "quality")]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use cordial::{
    GateLedger, ItemState, SessionBuilder, StoreLayout, TierSelection, Worklist, all_plugins,
    assigned_tier, etiquettes_from_plugins, load_cordial_config, project_slug_from_path,
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
    let panics = etiquettes
        .iter()
        .find(|etiquette| etiquette.id() == "panics")
        .ok_or_else(|| miette::miette!("panics unavailable"))?;
    assert_eq!(config.tiers().default(), &["build"]);
    assert!(defaults.includes(panics.as_ref(), &config));
    assert!(defaults.includes(antipatterns.as_ref(), &config));
    assert!(!defaults.includes(freshness.as_ref(), &config));

    let quick = ["quick".to_string()];
    let selected = TierSelection::resolve(&etiquettes, &config, Some(&quick)).into_diagnostic()?;
    assert!(selected.includes(panics.as_ref(), &config));
    assert!(!selected.includes(antipatterns.as_ref(), &config));
    assert!(!selected.includes(freshness.as_ref(), &config));

    let build = ["build".to_string()];
    let selected = TierSelection::resolve(&etiquettes, &config, Some(&build)).into_diagnostic()?;
    assert_eq!(selected.names().len(), 2);
    assert!(selected.includes(panics.as_ref(), &config));
    assert!(selected.includes(antipatterns.as_ref(), &config));
    assert!(!selected.includes(freshness.as_ref(), &config));

    let deep = ["deep".to_string()];
    let selected = TierSelection::resolve(&etiquettes, &config, Some(&deep)).into_diagnostic()?;
    assert_eq!(selected.names().len(), 3);
    assert!(selected.includes(panics.as_ref(), &config));
    assert!(selected.includes(antipatterns.as_ref(), &config));
    assert!(selected.includes(freshness.as_ref(), &config));

    let union = ["quick".to_string(), "deep".to_string()];
    let selected = TierSelection::resolve(&etiquettes, &config, Some(&union)).into_diagnostic()?;
    assert!(selected.includes(antipatterns.as_ref(), &config));
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
    assert!(!selected.includes(panics.as_ref(), &config));
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

#[test]
fn skipped_area_keeps_last_count_and_marks_checklist_stale() -> miette::Result<()> {
    cordial::init_tracing();
    let project = project()?;
    let home = tempfile::tempdir().into_diagnostic()?;
    fs::write(
        project.path().join("src/lib.rs"),
        "pub mod widgets;\npub fn root_item() {}\n",
    )
    .into_diagnostic()?;
    fs::write(
        project.path().join("src/widgets.rs"),
        "pub fn widget() {}\n",
    )
    .into_diagnostic()?;
    fs::write(
        project.path().join("cordial.toml"),
        "[tiers.assign]\nvisibility = \"deep\"\n",
    )
    .into_diagnostic()?;
    let first = run(
        project.path(),
        home.path(),
        &["quality", "--etiquette", "visibility"],
    )?;
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let store = store(project.path(), home.path());
    let ledger = GateLedger::read(&store).into_diagnostic()?;
    let previous = *ledger
        .area_open_counts()
        .get("Module visibility")
        .ok_or_else(|| miette::miette!("missing visibility count"))?;
    assert!(previous > 0);
    assert!(
        store
            .findings_dir()
            .join("visibility.checklist.md")
            .is_file()
    );

    let second = run(
        project.path(),
        home.path(),
        &["quality", "--tier", "quick", "--deny-open"],
    )?;
    assert!(
        String::from_utf8_lossy(&second.stderr).contains("Tiers not run: build, deep"),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let report =
        fs::read_to_string(store.findings_dir().join("quality-report.md")).into_diagnostic()?;
    let section = report
        .split("## ")
        .find(|section| section.contains("Module visibility\n"))
        .ok_or_else(|| miette::miette!("missing visibility section"))?;
    assert!(
        section.contains("not run (tier checks skipped: visibility: deep)"),
        "{section}"
    );
    assert!(
        section.contains(&format!("not run; {previous} at last full run")),
        "{section}"
    );
    assert!(
        section.contains("[`visibility.checklist.md`](visibility.checklist.md) (stale)"),
        "{section}"
    );
    assert!(!section.contains("**Open items:** 0"), "{section}");
    Ok(())
}

#[test]
fn deny_open_includes_items_from_omitted_tiers_in_text_and_json() -> miette::Result<()> {
    cordial::init_tracing();
    for command in ["submit", "quality"] {
        for state in [ItemState::Open, ItemState::Skipped, ItemState::Done] {
            for json in [false, true] {
                if command == "quality" && json {
                    continue;
                }
                let project = project()?;
                let home = tempfile::tempdir().into_diagnostic()?;
                fs::write(
                    project.path().join("cordial.toml"),
                    "[tiers.assign]\npanics = \"deep\"\nvisibility = \"focused\"\n",
                )
                .into_diagnostic()?;
                let initial = run(
                    project.path(),
                    home.path(),
                    &["quality", "--etiquette", "panics"],
                )?;
                assert!(initial.status.success(), "{initial:?}");
                let store = store(project.path(), home.path());
                let mut previous = Worklist::read(&store)
                    .into_diagnostic()?
                    .ok_or_else(|| miette::miette!("missing initial worklist"))?;
                assert!(!previous.items().is_empty());
                let ids = previous
                    .items()
                    .iter()
                    .map(|item| item.id().as_str().to_string())
                    .collect::<Vec<_>>();
                match state {
                    ItemState::Open => {}
                    ItemState::Skipped => {
                        previous.skip(&ids, Some("deferred")).into_diagnostic()?;
                    }
                    ItemState::Done => {
                        previous.done(&ids).into_diagnostic()?;
                    }
                }
                previous.write(&store).into_diagnostic()?;
                let mut args = vec![command, "--tier", "focused", "--deny-open"];
                if json {
                    args.push("--json");
                }
                let output = run(project.path(), home.path(), &args)?;
                let should_fail = state != ItemState::Done;
                assert_eq!(
                    output.status.success(),
                    !should_fail,
                    "{args:?} {state:?}: {output:?}"
                );
                let current = Worklist::read(&store)
                    .into_diagnostic()?
                    .ok_or_else(|| miette::miette!("missing reconciled worklist"))?;
                assert_eq!(
                    current.record().generation(),
                    &(previous.record().generation() + 1)
                );
                assert_eq!(current.items().len(), ids.len());
                assert_eq!(
                    current.open_count(),
                    if should_fail { ids.len() } else { 0 }
                );
                for item in current.items() {
                    assert!(ids.iter().any(|id| id == item.id().as_str()));
                    assert_eq!(*item.state(), state);
                    assert!(*item.unverified());
                    assert!(!*item.reopened());
                }
                if json {
                    // Parsing the whole stdout also rejects multiple JSON responses.
                    let response: serde_json::Value =
                        serde_json::from_slice(&output.stdout).into_diagnostic()?;
                    assert_eq!(response["command"], "submit");
                    if should_fail {
                        assert!(response["error"].is_object(), "{response}");
                    } else {
                        assert_eq!(response["gate"]["open_now"], 0);
                    }
                } else if command == "submit" {
                    assert!(
                        String::from_utf8_lossy(&output.stdout)
                            .contains(&format!("Gate: {} open items remain", current.open_count()))
                    );
                }
            }
        }
    }
    Ok(())
}
