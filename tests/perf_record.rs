//! Performance run records: their statistics, their identity, and their storage.

use cordial::StoreLayout;
use cordial::perf::{
    ChangeKind, PerfEnvironment, PerfIdentityBuilder, PerfState, PerfStore, PerfSubject, RunRecord,
    Spread, StageTiming, TestTiming,
};
use miette::{IntoDiagnostic, Result};
use std::collections::BTreeMap;

fn record(sha: &str, at: u64, subject: PerfSubject, state: Option<PerfState>) -> Result<RunRecord> {
    let identity = PerfIdentityBuilder::default()
        .sha(sha)
        .dirty(false)
        .branch(Some("dev".to_owned()))
        .recorded_at(at)
        .build()
        .into_diagnostic()?;
    Ok(RunRecord::new(
        identity,
        environment(),
        subject,
        state,
        vec![StageTiming::new("load".to_owned(), Spread::single(5))],
        Vec::new(),
        BTreeMap::from([("queries_executed".to_owned(), 7)]),
        Vec::new(),
    ))
}

fn environment() -> PerfEnvironment {
    PerfEnvironment::capture(std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
}

fn store() -> Result<(tempfile::TempDir, PerfStore)> {
    let home = tempfile::tempdir().into_diagnostic()?;
    let layout = StoreLayout::from_root(home.path().join("project"), "project");
    Ok((home, PerfStore::new(&layout)))
}

#[test]
fn spread_reports_median_extremes_and_deviation() -> Result<()> {
    cordial::init_tracing();
    let spread =
        Spread::from_samples(&[10, 12, 11, 100, 9]).ok_or_else(|| miette::miette!("no samples"))?;
    assert_eq!(spread.count(), 5);
    assert_eq!(spread.median(), 11);
    assert_eq!((spread.min(), spread.max()), (9, 100));
    assert_eq!(spread.mad(), 1, "one outlier does not move the deviation");
    let pair = Spread::from_samples(&[4, 8]).ok_or_else(|| miette::miette!("no samples"))?;
    assert_eq!(pair.median(), 6);
    assert!(Spread::from_samples(&[]).is_none());
    Ok(())
}

#[test]
fn a_record_survives_a_round_trip_through_the_store() -> Result<()> {
    cordial::init_tracing();
    let (_home, store) = store()?;
    let saved = record(
        "0123456789abcdef",
        1_700_000_000,
        PerfSubject::workflow("cordial-submit"),
        Some(PerfState::Cold),
    )?;
    let path = store.save(&saved).into_diagnostic()?;
    assert!(path.starts_with(store.dir()));
    assert_eq!(PerfStore::load(&path).into_diagnostic()?, saved);
    assert_eq!(
        saved.id(),
        "1700000000-01234567-workflow-cordial-submit-cold"
    );
    Ok(())
}

#[test]
fn records_list_oldest_first_and_are_never_overwritten() -> Result<()> {
    cordial::init_tracing();
    let (_home, store) = store()?;
    assert!(store.list().into_diagnostic()?.is_empty());
    let later = record("bbbbbbbb", 200, PerfSubject::tests("verus"), None)?;
    let earlier = record("aaaaaaaa", 100, PerfSubject::tests("verus"), None)?;
    store.save(&later).into_diagnostic()?;
    store.save(&earlier).into_diagnostic()?;
    let listed = store.list().into_diagnostic()?;
    assert_eq!(listed, vec![earlier.clone(), later]);
    assert!(store.save(&earlier).is_err(), "an existing id is refused");
    Ok(())
}

#[test]
fn a_record_of_an_unknown_format_is_refused_with_its_path() -> Result<()> {
    cordial::init_tracing();
    let (_home, store) = store()?;
    let saved = record("cccccccc", 300, PerfSubject::tests("x"), None)?;
    let path = store.save(&saved).into_diagnostic()?;
    let text = std::fs::read_to_string(&path).into_diagnostic()?;
    std::fs::write(&path, text.replacen("\"schema\": 1", "\"schema\": 99", 1)).into_diagnostic()?;
    let message = match PerfStore::load(&path) {
        Ok(_) => return Err(miette::miette!("an unknown format version was accepted")),
        Err(error) => error.to_string(),
    };
    assert!(message.contains("format version 99"), "{message}");
    assert!(
        message.contains(path.to_string_lossy().as_ref()),
        "{message}"
    );
    assert!(
        store.list().is_err(),
        "a bad record is not silently skipped"
    );
    Ok(())
}

#[test]
fn an_environment_that_differs_is_still_comparable_and_says_what_differs() -> Result<()> {
    cordial::init_tracing();
    let here = environment();
    assert!(here.changes_from(&environment()).is_empty());
    assert_eq!(here.machine(), environment().machine());

    let mut changed = serde_json::to_value(&here).into_diagnostic()?;
    changed["rustc"] = serde_json::json!("rustc 0.0.0 (elsewhere)");
    changed["dependencies"]["ra_ap_hir"] = serde_json::json!("0.0.1");
    let changed: PerfEnvironment = serde_json::from_value(changed).into_diagnostic()?;
    let changes = here.changes_from(&changed);
    let kinds: Vec<_> = changes
        .iter()
        .map(|change| (change.kind(), change.name().as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            (ChangeKind::Software, "rustc"),
            (ChangeKind::Dependency, "ra_ap_hir")
        ]
    );
    assert_eq!(changes[1].before(), "0.0.1");
    assert_eq!(changed.machine(), here.machine(), "software is not machine");

    let mut other_machine = serde_json::to_value(&here).into_diagnostic()?;
    other_machine["cores"] = serde_json::json!(here.cores() + 1);
    let other_machine: PerfEnvironment = serde_json::from_value(other_machine).into_diagnostic()?;
    assert_ne!(other_machine.machine(), here.machine());
    assert_eq!(
        here.changes_from(&other_machine)[0].kind(),
        ChangeKind::Machine
    );
    Ok(())
}

#[test]
fn the_locked_dependencies_are_recorded() {
    cordial::init_tracing();
    assert!(environment().dependencies().contains_key("ra_ap_hir"));
    let nowhere = PerfEnvironment::capture(std::path::Path::new("/nonexistent"));
    assert!(nowhere.dependencies().is_empty());
}

#[test]
fn the_recorded_rust_analyzer_release_matches_cargo_toml() -> Result<()> {
    cordial::init_tracing();
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .into_diagnostic()?;
    assert!(
        manifest.contains(&format!(
            "ra_ap_hir = \"{}\"",
            environment().ra_ap_version()
        )),
        "update RA_AP_VERSION when the ra_ap_* cohort moves"
    );
    Ok(())
}

#[test]
fn a_tests_fixed_share_is_setup_over_wall_time() {
    cordial::init_tracing();
    let timing = TestTiming::new(
        "binary".to_owned(),
        "test".to_owned(),
        Spread::single(1000),
        Vec::new(),
        800,
        BTreeMap::new(),
    );
    assert!((timing.fixed_share() - 0.8).abs() < 1e-9);
}

#[test]
fn cleaning_leaves_performance_records_in_place() -> Result<()> {
    cordial::init_tracing();
    let home = tempfile::tempdir().into_diagnostic()?;
    let layout = StoreLayout::from_root(home.path().join("project"), "project");
    assert!(layout.curated_paths().contains(&layout.perf_dir()));
    assert!(!layout.regenerable_paths().contains(&layout.perf_dir()));
    Ok(())
}
