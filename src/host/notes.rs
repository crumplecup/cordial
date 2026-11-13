//! What the databases did during the current run, for the run log: whether one
//! was loaded, reused, or reloaded because the disk no longer matched it.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, PoisonError};
use tracing::instrument;

static LOADED: AtomicU32 = AtomicU32::new(0);
static KEPT: AtomicU32 = AtomicU32::new(0);
static RELOADED: AtomicU32 = AtomicU32::new(0);
static FILES_CHANGED: AtomicU32 = AtomicU32::new(0);
static CACHE_HITS: AtomicU32 = AtomicU32::new(0);
static EVENTS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static LAST_EVENTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// What happened to the databases since the last [`take`].
#[derive(Debug, Clone, Default, PartialEq, Eq, derive_getters::Getters)]
pub struct RunNotes {
    /// Workspaces loaded from scratch.
    #[getter(copy)]
    loaded: u32,
    /// Kept databases reused.
    #[getter(copy)]
    kept: u32,
    /// Kept databases dropped because the disk no longer matched them.
    #[getter(copy)]
    reloaded: u32,
    /// Source files fed into kept databases as edits.
    #[getter(copy)]
    files_changed: u32,
    /// Targets answered from the run cache without a database.
    #[getter(copy)]
    cache_hits: u32,
    /// Each cache and database decision the run made, in the order it made it,
    /// in words: what it reused, what it analysed again, and why.
    events: Vec<String>,
}

/// A workspace was loaded from scratch.
#[instrument(level = "trace")]
pub(crate) fn loaded() {
    LOADED.fetch_add(1, Ordering::Relaxed);
}

/// A kept database was reused after `files` source files were brought up to date.
#[instrument(level = "trace")]
pub(crate) fn kept(files: usize) {
    KEPT.fetch_add(1, Ordering::Relaxed);
    FILES_CHANGED.fetch_add(u32::try_from(files).unwrap_or(u32::MAX), Ordering::Relaxed);
}

/// A kept database could not follow the disk and was dropped.
#[instrument(level = "trace")]
pub(crate) fn reloaded() {
    RELOADED.fetch_add(1, Ordering::Relaxed);
}

/// A target was answered from the run cache.
#[instrument(level = "trace")]
pub(crate) fn cache_hit() {
    CACHE_HITS.fetch_add(1, Ordering::Relaxed);
}

/// Say what the run decided about reuse. These are the program's own account of
/// whether it used a cache, kept for the run log and shown when a run is stopped.
#[instrument(level = "debug")]
pub(crate) fn event(text: String) {
    tracing::info!(target: "cordial::cache", "{text}");
    EVENTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(text);
}

/// The events of the run that ended last, for a caller that prints them.
#[instrument(level = "debug")]
pub fn last_events() -> Vec<String> {
    LAST_EVENTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Everything noted since the last call, resetting the counts.
#[instrument(level = "debug")]
pub fn take() -> RunNotes {
    let events = std::mem::take(&mut *EVENTS.lock().unwrap_or_else(PoisonError::into_inner));
    *LAST_EVENTS.lock().unwrap_or_else(PoisonError::into_inner) = events.clone();
    RunNotes {
        loaded: LOADED.swap(0, Ordering::Relaxed),
        kept: KEPT.swap(0, Ordering::Relaxed),
        reloaded: RELOADED.swap(0, Ordering::Relaxed),
        files_changed: FILES_CHANGED.swap(0, Ordering::Relaxed),
        cache_hits: CACHE_HITS.swap(0, Ordering::Relaxed),
        events,
    }
}
