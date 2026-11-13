//! The loaded databases a session keeps between runs.
//!
//! A run that needs the loaded workspace asks here first. A database kept from
//! an earlier run is brought up to date with the files on disk and reused; one
//! that cannot be (the Cargo graph changed, a source file came or went) is
//! dropped and the workspace is loaded again. Outside a session nothing is kept.

use crate::loader::{SourceDatabase, SyncOutcome};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use tracing::instrument;

static KEEPING: AtomicBool = AtomicBool::new(false);
static KEPT: Mutex<Option<HashMap<String, SourceDatabase>>> = Mutex::new(None);

/// Start keeping databases between runs: this process is a session.
#[instrument(level = "debug")]
#[hotpath::measure]
pub fn keep_between_runs() {
    KEEPING.store(true, Ordering::Relaxed);
}

/// Whether this process keeps databases between runs.
#[instrument(level = "trace")]
pub fn keeping() -> bool {
    KEEPING.load(Ordering::Relaxed)
}

/// What a database needs for a workspace identity: the project and the options
/// that shape its Cargo graph, in one string.
#[instrument(level = "trace", skip(options))]
pub fn key_of(root: &std::path::Path, options: &impl std::fmt::Debug) -> String {
    format!("{}|{options:?}", root.display())
}

/// Take the kept database for `key`, brought up to date with the disk. `None`
/// when there is none, or it could not follow the disk and was dropped.
#[instrument(level = "debug", skip(self_key))]
#[hotpath::measure]
pub fn take_current(self_key: &str) -> Option<SourceDatabase> {
    let mut database = KEPT
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_mut()
        .and_then(|kept| kept.remove(self_key))?;
    match database.sync_from_disk() {
        Ok(SyncOutcome::Current) => {
            tracing::info!(target: "cordial::session", "reusing the kept database; nothing changed");
            super::kept(0);
            super::event("database: reused from the session, no source file changed".to_owned());
            Some(database)
        }
        Ok(SyncOutcome::Updated(files)) => {
            tracing::info!(
                target: "cordial::session",
                files = files.len(),
                "reusing the kept database; source files changed"
            );
            super::kept(files.len());
            super::event(format!(
                "database: reused from the session; {} source file(s) changed and were fed in",
                files.len()
            ));
            Some(database)
        }
        Ok(SyncOutcome::Reload(path)) => {
            tracing::info!(
                target: "cordial::session",
                path = %path.display(),
                "the kept database no longer matches the disk; loading again"
            );
            super::reloaded();
            super::event(format!(
                "database: dropped, {} changed what it was loaded from; loading again",
                path.display()
            ));
            None
        }
        Err(error) => {
            tracing::warn!(target: "cordial::session", error = ?error, "could not bring the kept database up to date; loading again");
            super::reloaded();
            super::event(
                "database: dropped, it could not follow the disk; loading again".to_owned(),
            );
            None
        }
    }
}

/// Keep `database` for the next run that asks for `key`.
#[instrument(level = "debug", skip(database))]
#[hotpath::measure]
pub fn keep(key: String, database: SourceDatabase) {
    KEPT.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .insert(key, database);
}
