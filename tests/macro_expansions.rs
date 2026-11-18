//! What a declarative macro, a built-in derive or a built-in attribute expands to is a saved query,
//! like a proc macro's: a cold run expands them, and a run restored from its state expands none, which
//! is also what lets them go to the cold store when memory is short.

use cordial::{
    CrateTarget, MetricsLayer, SavedState, SemanticCargoOptions, SourceDatabase, StoreLayout,
    read_run_metrics, write_run_metrics,
};
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;

const LIBRARY: &str = "\
macro_rules! double { ($x:expr) => { $x + $x }; }

#[derive(Debug, Clone, PartialEq)]
pub struct Point { x: i32 }

pub fn doubled() -> i32 { double!(2) }

pub fn point() -> Point { Point { x: doubled() }.clone() }
";

/// How many times the query ran, from the metrics of the process so far.
fn expansions_so_far() -> Result<u64> {
    let store = tempfile::tempdir().into_diagnostic()?;
    let layout = StoreLayout::from_root(store.path().to_path_buf(), "expanded".to_owned());
    write_run_metrics(&layout).into_diagnostic()?;
    Ok(read_run_metrics(&layout)
        .values()
        .iter()
        .filter(|(key, _)| key.starts_with("steps.") && key.ends_with("> expand_pure_macro_of"))
        .map(|(_, count)| *count)
        .sum())
}

#[test]
fn a_restored_run_expands_none_of_the_macros_a_cold_run_expanded() -> Result<()> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='expanded'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    let subscriber = tracing_subscriber::registry()
        .with(MetricsLayer.with_filter(cordial::metrics_filter_with(true)));
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    let options = SemanticCargoOptions::default();

    let run = || -> Result<()> {
        let mut session = SourceDatabase::load_saved(root.path(), &options, Some(saved.clone()))
            .into_diagnostic()?;
        session
            .project(
                &CrateTarget::new("expanded", root.path()),
                &[root.path().join("src/lib.rs")],
            )
            .into_diagnostic()?;
        Ok(())
    };

    run()?;
    let cold = expansions_so_far()?;
    assert!(
        cold > 0,
        "a cold run expands the macro and the derives: {cold}"
    );
    run()?;
    assert_eq!(
        expansions_so_far()? - cold,
        0,
        "a restored run expands none: they are in the state"
    );
    Ok(())
}
