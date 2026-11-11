//! A run reuses the results of an earlier run while its inputs are unchanged.

use cordial::{PANICS_ETIQUETTE, ProgressSink, ProgressTask, RunAll, Session, SessionBuilder};
use miette::{IntoDiagnostic, Result};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

const LIBRARY: &str = include_str!("fixtures/result_cache.rs");
const EDITED: &str = include_str!("fixtures/result_cache_edited.rs");

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Every message a run shows the user, in order, and how far its bars advanced.
#[derive(Default)]
struct Messages {
    shown: Arc<Mutex<Vec<String>>>,
    advanced: Arc<AtomicU64>,
}

struct Recorded {
    shown: Arc<Mutex<Vec<String>>>,
    advanced: Arc<AtomicU64>,
}

impl ProgressSink for Messages {
    fn spinner(&self, message: String) -> Box<dyn ProgressTask> {
        locked(&self.shown).push(message);
        Box::new(Recorded {
            shown: self.shown.clone(),
            advanced: self.advanced.clone(),
        })
    }

    fn bar(&self, message: String, len: u64) -> Box<dyn ProgressTask> {
        self.spinner(format!("{message} ({len} steps)"))
    }
}

impl ProgressTask for Recorded {
    fn set_message(&self, message: String) {
        locked(&self.shown).push(message);
    }

    fn inc(&self, delta: u64) {
        self.advanced.fetch_add(delta, Ordering::Relaxed);
    }

    fn finish(&self, message: String) {
        locked(&self.shown).push(message);
    }
}

fn fixture() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='cached'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    Ok(root)
}

/// Run once against `store` and return the messages shown and the open findings.
fn run(root: &std::path::Path, store: &std::path::Path) -> Result<(Vec<String>, usize)> {
    let messages = Messages::default();
    let log = messages.shown.clone();
    let advanced = messages.advanced.clone();
    let session = SessionBuilder::new(root)
        .with_store_root(store)
        .with_progress_sink(Arc::new(messages))
        .register(&*PANICS_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic()?;
    let open = outcome
        .findings()
        .filter(|finding| finding.disposition() == cordial::Disposition::Open)
        .count();
    assert!(
        advanced.load(Ordering::Relaxed) > 0,
        "the run's bar advanced"
    );
    let shown = locked(&log).clone();
    Ok((shown, open))
}

fn any(messages: &[String], needle: &str) -> bool {
    messages.iter().any(|message| message.contains(needle))
}

#[test]
fn a_repeat_run_reuses_the_analysis_and_a_changed_source_does_not() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let store = tempfile::tempdir().into_diagnostic()?;

    let (first, first_open) = run(root.path(), store.path())?;
    assert!(
        any(&first, "Loading the workspace into rust-analyzer"),
        "{first:?}"
    );
    assert!(any(&first, "no earlier analysis"), "{first:?}");

    let (second, second_open) = run(root.path(), store.path())?;
    assert!(
        any(&second, "Reusing the cached analysis of cached"),
        "{second:?}"
    );
    assert!(
        !any(&second, "Loading the workspace into rust-analyzer"),
        "a hit must not load the workspace: {second:?}"
    );
    assert_eq!(
        first_open, second_open,
        "a hit reports what a full run reports"
    );

    std::fs::write(root.path().join("src/lib.rs"), EDITED).into_diagnostic()?;
    let (third, third_open) = run(root.path(), store.path())?;
    assert!(any(&third, "input(s) changed"), "{third:?}");
    assert!(
        any(&third, "src/lib.rs"),
        "the message names what changed: {third:?}"
    );
    assert!(
        any(&third, "Loading the workspace into rust-analyzer"),
        "{third:?}"
    );
    assert_eq!(
        third_open,
        second_open + 1,
        "the edit's new panic site is found"
    );
    Ok(())
}
