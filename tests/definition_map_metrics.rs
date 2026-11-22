//! What a run says about building the definition maps: which crates, and for each what it cost, so
//! the load of a workspace can be taken apart by crate from the run's metrics alone, and the
//! memory at each step of the load is kept even for a run that does not finish.

use cordial::{
    CrateTarget, LiveLayer, MetricsLayer, SemanticCargoOptions, SourceDatabase, StoreLayout,
    live_filter, metrics_filter_with,
};
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::{Layer, layer::SubscriberExt};

const APP: &str = "\
macro_rules! twice { ($e:expr) => { $e + $e }; }
pub fn base() -> u32 { twice!(1) }
pub use dependency::plain;
";

const DEPENDENCY: &str = "pub fn plain() {}\n";

fn fixture() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    for dir in ["src", "dependency/src"] {
        std::fs::create_dir_all(root.path().join(dir)).into_diagnostic()?;
    }
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n\
         [dependencies]\ndependency = { path = 'dependency' }\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), APP).into_diagnostic()?;
    std::fs::write(
        root.path().join("dependency/Cargo.toml"),
        "[package]\nname='dependency'\nversion='0.1.0'\nedition='2024'\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("dependency/src/lib.rs"), DEPENDENCY).into_diagnostic()?;
    Ok(root)
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_load_says_what_each_definition_map_cost_and_the_memory_at_each_step() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    // As a run installs them: the metrics, and the counts of the queries Salsa executes.
    let subscriber = tracing_subscriber::registry()
        .with(MetricsLayer.with_filter(metrics_filter_with(false)))
        .with(LiveLayer.with_filter(live_filter()));
    tracing::subscriber::with_default(subscriber, || -> Result<()> {
        let mut session = SourceDatabase::load_with(root.path(), &SemanticCargoOptions::default())
            .into_diagnostic()?;
        session
            .project(
                &CrateTarget::new("app", root.path()),
                &[root.path().join("src/lib.rs")],
            )
            .into_diagnostic()?;
        Ok(())
    })?;
    let store = StoreLayout::from_root(root.path().join("store"), "specimen".to_owned());
    cordial::write_run_metrics(&store).into_diagnostic()?;
    let metrics = cordial::read_run_metrics(&store);
    let value = |name: &str| metrics.values().get(name).copied();

    // The crates chosen, of all the crates the database holds, by where they come from.
    let chosen = value("definition_maps.chosen_first.crates").unwrap_or(0);
    assert!(
        chosen >= 2,
        "the package, its dependency and the toolchain: {chosen}"
    );
    assert!(value("definition_maps.chosen_first.everything").unwrap_or(0) >= chosen);
    assert_eq!(value("definition_maps.chosen_first.local"), Some(2));
    assert!(value("definition_maps.chosen_first.sysroot").unwrap_or(0) > 0);

    // What each crate's definition map cost: its source, its time, what it executed.
    assert_eq!(value("definition_maps.app_first.local"), Some(1));
    assert!(value("definition_maps.app_first.source_kib").is_some());
    assert!(value("definition_maps.app_first.micros").unwrap_or(0) > 0);
    assert!(value("definition_maps.app_first.queries").unwrap_or(0) > 0);
    assert!(value("definition_maps.app_first.resident_kib").unwrap_or(0) > 0);
    assert!(value("definition_maps.app_first.expansions").is_some());
    assert!(value("definition_maps.app_first.elapsed_ms").is_some());
    assert_eq!(value("definition_maps.dependency_first.local"), Some(1));
    assert_eq!(value("definition_maps.core_first.sysroot"), Some(1));
    assert!(value("definition_maps.built_first.micros").unwrap_or(0) > 0);

    // The memory at each step of the load, with when it was and what the machine had left.
    for step in [
        "cargo_metadata_read",
        "build_scripts_run",
        "rust_analyzer_database_loaded",
        "workspace_loaded",
        "definition_maps_built_call_first",
    ] {
        assert!(
            value(&format!("milestones.{step}.resident_kib")).unwrap_or(0) > 0,
            "{step}"
        );
        assert!(
            value(&format!("milestones.{step}.available_kib")).unwrap_or(0) > 0,
            "{step}"
        );
        assert!(
            value(&format!("milestones.{step}.elapsed_ms")).is_some(),
            "{step}"
        );
    }
    Ok(())
}
