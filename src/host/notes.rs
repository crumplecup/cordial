//! What the databases did during the current run, for the run log: whether one
//! was loaded, reused, or reloaded because the disk no longer matched it.

use std::sync::atomic::{AtomicU32, Ordering};
use tracing::instrument;

static LOADED: AtomicU32 = AtomicU32::new(0);
static KEPT: AtomicU32 = AtomicU32::new(0);
static RELOADED: AtomicU32 = AtomicU32::new(0);
static FILES_CHANGED: AtomicU32 = AtomicU32::new(0);
static CACHE_HITS: AtomicU32 = AtomicU32::new(0);

/// What happened to the databases since the last [`take`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, derive_getters::Getters)]
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

/// Everything noted since the last call, resetting the counts.
#[instrument(level = "debug")]
pub fn take() -> RunNotes {
    RunNotes {
        loaded: LOADED.swap(0, Ordering::Relaxed),
        kept: KEPT.swap(0, Ordering::Relaxed),
        reloaded: RELOADED.swap(0, Ordering::Relaxed),
        files_changed: FILES_CHANGED.swap(0, Ordering::Relaxed),
        cache_hits: CACHE_HITS.swap(0, Ordering::Relaxed),
    }
}
