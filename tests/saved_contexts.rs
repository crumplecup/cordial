//! A cfg context's crates and file copies are saved with the database and taken up again by
//! the next run, so what was computed for them is not computed again.

mod support {
    pub mod semantic_metrics;
}

use cordial::{
    CfgPlanning, CrateIr, CrateTarget, IrView, SavedState, SemanticCargoOptions,
    SemanticContextKey, SourceDatabase,
};
use miette::{IntoDiagnostic, Result};
use std::path::Path;
use tracing_subscriber::prelude::*;

const LIBRARY: &str = "\
pub fn ordinary() {}

#[cfg(kani)]
pub fn harness() { ordinary(); }

#[cfg(not(kani))]
pub fn unless_kani() {}
";

fn fixture() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='gated'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    Ok(root)
}

fn run(root: &Path, saved: &SavedState) -> Result<(bool, CrateIr)> {
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(vec![SemanticContextKey::enabling(["kani"])])
        .with_cfg_planning(CfgPlanning::Explicit);
    let mut session =
        SourceDatabase::load_saved(root, &options, Some(saved.clone())).into_diagnostic()?;
    let restored = session.restored();
    let graph = session
        .project(&CrateTarget::new("gated", root), &[root.join("src/lib.rs")])
        .into_diagnostic()?;
    Ok((restored, graph))
}

fn functions(graph: &CrateIr) -> Vec<String> {
    let mut names: Vec<String> = graph
        .nodes_matching(&cordial::BasicQuery::items())
        .into_iter()
        .filter_map(|node| node.weight().name().clone())
        .collect();
    names.sort();
    names
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_restored_run_takes_the_contexts_up_again_and_computes_nothing_for_them() -> Result<()> {
    cordial::init_tracing();
    let metrics = support::semantic_metrics::QueryMetrics::default();
    let subscriber = tracing_subscriber::registry()
        .with(metrics.clone())
        .with(cordial::tracing_fmt_layer());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = fixture()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));

    let (restored, cold) = run(root.path(), &saved)?;
    assert!(!restored);
    let cold_counts = metrics.snapshot();
    assert!(!cold_counts.is_empty());

    let (restored, warm) = run(root.path(), &saved)?;
    assert!(restored, "the second run starts from the first's state");
    assert_eq!(functions(&warm), functions(&cold), "the same answer");
    // What the restored run computed, by query.
    let mut recomputed: std::collections::BTreeMap<String, usize> = metrics
        .snapshot()
        .into_iter()
        .filter_map(|(key, count)| {
            let before = cold_counts.get(&key).copied().unwrap_or(0);
            (count > before).then(|| (key, count - before))
        })
        .collect();
    // The joins over a crate's files are not saved; they are rebuilt from the saved parts.
    const JOINS: [&str; 5] = [
        "antipattern_crate_facts:",
        "architecture_catalog:",
        "panic_graph:",
        "panic_reachable:",
        "signature_index:",
    ];
    recomputed.retain(|key, _| !JOINS.iter().any(|join| key.starts_with(join)));
    assert!(
        recomputed.is_empty(),
        "the restored run computed again: {recomputed:#?}"
    );
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn an_edit_between_runs_reaches_the_taken_up_contexts() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    let (_, cold) = run(root.path(), &saved)?;
    assert!(functions(&cold).contains(&"harness".to_owned()));

    // A function gated on kani is added; the context's copy of the file follows the edit.
    let library = root.path().join("src/lib.rs");
    std::fs::write(
        &library,
        format!("{LIBRARY}\n#[cfg(kani)]\npub fn second_harness() {{ ordinary(); }}\n"),
    )
    .into_diagnostic()?;
    let (restored, warm) = run(root.path(), &saved)?;
    assert!(restored);
    let names = functions(&warm);
    assert!(
        names.contains(&"second_harness".to_owned()),
        "the context sees the edit: {names:?}"
    );
    assert!(names.contains(&"harness".to_owned()));
    Ok(())
}

/// A workspace of two local crates, one depending on the other, each with code gated on cfg.
fn two_crates() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    for directory in ["src", "inner/src"] {
        std::fs::create_dir_all(root.path().join(directory)).into_diagnostic()?;
    }
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='gated'\nversion='0.1.0'\nedition='2024'\n\
         [dependencies]\ninner = { path = \"inner\" }\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("inner/Cargo.toml"),
        "[package]\nname='inner'\nversion='0.1.0'\nedition='2024'\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("inner/src/lib.rs"),
        "pub fn helper() {}\n#[cfg(kani)]\npub fn kani_only() { helper(); }\n\
         #[cfg(creusot)]\npub fn creusot_only() { helper(); }\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("src/lib.rs"),
        "pub fn ordinary() { inner::helper(); }\n#[cfg(kani)]\npub fn harness() { inner::kani_only(); }\n\
         #[cfg(creusot)]\npub fn proof() { inner::creusot_only(); }\n",
    )
    .into_diagnostic()?;
    Ok(root)
}

fn run_many(root: &Path, saved: &SavedState) -> Result<bool> {
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(vec![
            SemanticContextKey::enabling(["kani"]),
            SemanticContextKey::enabling(["creusot"]),
        ])
        .with_cfg_planning(CfgPlanning::Explicit);
    let mut session =
        SourceDatabase::load_saved(root, &options, Some(saved.clone())).into_diagnostic()?;
    let restored = session.restored();
    session
        .project(&CrateTarget::new("gated", root), &[root.join("src/lib.rs")])
        .into_diagnostic()?;
    Ok(restored)
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_restored_run_reuses_what_several_contexts_over_dependent_crates_computed() -> Result<()> {
    cordial::init_tracing();
    let metrics = support::semantic_metrics::QueryMetrics::default();
    let subscriber = tracing_subscriber::registry()
        .with(metrics.clone())
        .with(cordial::tracing_fmt_layer());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = two_crates()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));

    assert!(!run_many(root.path(), &saved)?);
    let cold_counts = metrics.snapshot();
    assert!(!cold_counts.is_empty());

    assert!(
        run_many(root.path(), &saved)?,
        "the second run starts from the first's state"
    );
    const JOINS: [&str; 5] = [
        "antipattern_crate_facts:",
        "architecture_catalog:",
        "panic_graph:",
        "panic_reachable:",
        "signature_index:",
    ];
    let recomputed: std::collections::BTreeMap<String, usize> = metrics
        .snapshot()
        .into_iter()
        .filter_map(|(key, count)| {
            let before = cold_counts.get(&key).copied().unwrap_or(0);
            (count > before && !JOINS.iter().any(|join| key.starts_with(join)))
                .then(|| (key, count - before))
        })
        .collect();
    assert!(
        recomputed.is_empty(),
        "the restored run computed again: {recomputed:#?}"
    );
    Ok(())
}
