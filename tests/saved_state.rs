//! A database saved by one run is restored by the next, whatever the next run's edits; a state
//! that cannot be used is ignored and the run goes on cold.

#[path = "support/identity_specimen.rs"]
mod identity_specimen;

use std::path::Path;

use cordial::{SavedState, SemanticCargoOptions, SemanticItemId, SourceDatabase};
use identity_specimen::{caller_identities, paths, project, specimen};
use miette::{IntoDiagnostic, Result};

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
