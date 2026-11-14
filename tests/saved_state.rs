//! A database saved by one run is restored by the next, whatever the next run's edits; a state
//! that cannot be used is ignored and the run goes on cold.

#[path = "support/identity_specimen.rs"]
mod identity_specimen;

use std::path::Path;

use cordial::{SavedState, SemanticCargoOptions, SemanticItemId, SourceDatabase};
use identity_specimen::{caller_identities, paths, project, specimen};
use miette::{IntoDiagnostic, Result};

/// How many functions the run evaluated, from the line it logs (the process-wide notes are
/// taken, so each call reads the run since the last).
fn evaluated() -> Option<usize> {
    let notes = cordial::host::take_notes();
    notes.events().iter().find_map(|event| {
        let rest = event.strip_prefix("function evidence (workspace): ")?;
        rest.split_whitespace().next()?.parse().ok()
    })
}

/// One run: load (restoring when there is a state), project, and say what `caller` calls.
fn run(
    root: &Path,
    saved: &SavedState,
) -> Result<(bool, Vec<SemanticItemId>, Vec<SemanticItemId>)> {
    let mut session =
        SourceDatabase::load_saved(root, &SemanticCargoOptions::default(), Some(saved.clone()))
            .into_diagnostic()?;
    let restored = session.restored();
    let graph = project(&mut session, root)?;
    let (targets, types) = caller_identities(&graph)?;
    Ok((restored, targets, types))
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_saved_database_is_restored_and_gives_every_definition_the_same_id() -> Result<()> {
    cordial::init_tracing();
    let root = specimen()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));

    let (restored, first_targets, first_types) = run(root.path(), &saved)?;
    assert!(!restored, "nothing was saved yet");
    assert!(
        saved.directory().join("key").is_file(),
        "the first run saved its database"
    );

    let (restored, targets, types) = run(root.path(), &saved)?;
    assert!(restored, "the second run starts from what the first saved");
    // The ids are the interned table's, and the table was restored: the same definitions,
    // the same ids, in a database that never saw the first.
    assert_eq!(targets, first_targets);
    assert_eq!(types, first_types);
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn an_edit_between_runs_keeps_the_ids_of_what_it_left_alone() -> Result<()> {
    cordial::init_tracing();
    let root = specimen()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    let (_, first_targets, _) = run(root.path(), &saved)?;

    // A function added after `alpha` moves nothing before it.
    let second = root.path().join("src/second.rs");
    std::fs::write(&second, "pub fn alpha() {}\npub fn beta() {}\n").into_diagnostic()?;
    let (restored, targets, _) = run(root.path(), &saved)?;
    assert!(restored, "a changed file does not stop the restore");
    assert_eq!(targets, first_targets, "the callees did not move");
    assert_eq!(paths(root.path()).len(), 3);
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_state_that_cannot_be_used_is_ignored_and_the_run_goes_on_cold() -> Result<()> {
    cordial::init_tracing();
    let root = specimen()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    let (_, first_targets, _) = run(root.path(), &saved)?;

    // Written by another build of cordial.
    std::fs::write(saved.directory().join("key"), "another build").into_diagnostic()?;
    let (restored, targets, _) = run(root.path(), &saved)?;
    assert!(!restored, "another build's state is not used");
    assert_eq!(
        targets.len(),
        first_targets.len(),
        "and the run still works"
    );

    // Damaged.
    let (_, _, _) = run(root.path(), &saved)?; // saves a good state again
    std::fs::write(saved.directory().join("salsa"), b"not a database").into_diagnostic()?;
    let (restored, targets, _) = run(root.path(), &saved)?;
    assert!(!restored, "a damaged state is not used");
    assert_eq!(targets.len(), first_targets.len());

    // Missing entirely.
    std::fs::remove_dir_all(saved.directory()).into_diagnostic()?;
    let (restored, _, _) = run(root.path(), &saved)?;
    assert!(!restored);
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_restored_run_does_not_compute_the_evidence_it_was_saved_with() -> Result<()> {
    cordial::init_tracing();
    let root = specimen()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    let _ = evaluated();

    let (_, first_targets, _) = run(root.path(), &saved)?;
    let cold = evaluated().ok_or_else(|| miette::miette!("the cold run logs its evaluation"))?;
    assert!(
        cold > 0,
        "a cold run computes the evidence of the crate's functions"
    );

    // Nothing changed: the saved evidence answers. What is computed is the two `helper`s a
    // macro generates, which no description can find again, so they have no saved evidence.
    let (restored, targets, _) = run(root.path(), &saved)?;
    assert!(restored);
    assert_eq!(
        evaluated(),
        Some(2),
        "only the macro-generated functions are computed again; the other {} are reused",
        cold - 2
    );
    assert_eq!(targets, first_targets);

    // An edit to one file: what reads it is computed again, and the answer is right.
    let first = root.path().join("src/first.rs");
    std::fs::write(&first, "pub fn alpha() {}\n// an edit\n").into_diagnostic()?;
    let (restored, targets, _) = run(root.path(), &saved)?;
    assert!(restored);
    assert!(
        evaluated().is_some_and(|count| count > 0),
        "an edit invalidates the evidence that read the file"
    );
    assert_eq!(
        targets, first_targets,
        "and the callees are the same definitions"
    );
    Ok(())
}

mod support {
    pub mod semantic_metrics;
}

/// What a restored run must not compute: each of the per-file queries the projection runs.
const PER_FILE: [&str; 17] = [
    "file_facts:",
    "file_items:",
    "error_sites_file:",
    "error_chains_file:",
    "error_types_file:",
    "error_compliance_file:",
    "error_apply_file:",
    "observability_file:",
    "visibility_file:",
    "policy_file:",
    "verus_file:",
    "antipattern_facts_file:",
    "antipattern_sites_file:",
    "panic_proofs_file:",
    "panic_scan_file:",
    "architecture_file:",
    "signature_file:",
];

fn executed(metrics: &support::semantic_metrics::QueryMetrics, family: &str) -> usize {
    metrics
        .snapshot()
        .iter()
        .filter(|(key, _)| key.starts_with(family))
        .map(|(_, count)| *count)
        .sum()
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_restored_run_computes_none_of_the_per_file_collectors() -> Result<()> {
    use tracing_subscriber::prelude::*;
    cordial::init_tracing();
    let metrics = support::semantic_metrics::QueryMetrics::default();
    let subscriber = tracing_subscriber::registry()
        .with(metrics.clone())
        .with(cordial::tracing_fmt_layer());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = specimen()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));

    run(root.path(), &saved)?;
    let cold: Vec<usize> = PER_FILE
        .iter()
        .map(|family| executed(&metrics, family))
        .collect();
    assert!(
        cold.iter().all(|count| *count > 0),
        "every family runs in a cold run: {cold:?}"
    );

    let (restored, _, _) = run(root.path(), &saved)?;
    assert!(restored);
    for (family, before) in PER_FILE.iter().zip(&cold) {
        assert_eq!(
            executed(&metrics, family),
            *before,
            "{family} ran again in the restored run"
        );
    }
    Ok(())
}
