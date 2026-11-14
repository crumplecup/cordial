//! A database saved at a checkpoint of one run and restored at the start of the next.
//!
//! Salsa's persistence saves the memos with their dependency edges, and the inputs those
//! edges end at; restoring them into an empty database and then loading the workspace onto
//! it brings the inputs up to date, and Salsa invalidates exactly what the difference
//! reaches. Nothing stays loaded between commands: the state is a directory on disk, written
//! by one process and read by the next.
//!
//! The state is only meaningful to the build of cordial that wrote it (the facts and the
//! descriptions in it are that build's), so it carries that build's fingerprint and is
//! ignored by any other. Anything that goes wrong reading it is reported and the run starts
//! cold; a saved state never decides whether a run succeeds.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use ra_ap_ide_db::RootDatabase;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use tracing::instrument;

use crate::{CordialError, CordialResult};

const KEY: &str = "key";
const SALSA: &str = "salsa";
const FILES: &str = "files";
const CRATES: &str = "crates";

/// Where a database is saved between runs.
#[derive(Debug, Clone)]
pub struct SavedState {
    dir: PathBuf,
}

impl SavedState {
    /// The state kept in `dir`.
    #[instrument(level = "trace", skip(dir))]
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Save `database`, replacing what was saved. Written beside the state and moved into
    /// place, with the key last, so a reader never sees half of one. Returns the bytes
    /// written.
    #[instrument(level = "debug", skip(self, database), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SavedState")]
    pub(super) fn save(&self, database: &mut RootDatabase) -> CordialResult<usize> {
        let name = self
            .dir
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let writing = self
            .dir
            .with_file_name(format!("{name}.writing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&writing);
        std::fs::create_dir_all(&writing)?;
        let files =
            postcard::to_stdvec(&database.files().dump()).map_err(|error| self.encoding(&error))?;
        let crates = postcard::to_stdvec(&database.crates_map().dump())
            .map_err(|error| self.encoding(&error))?;
        let database: &mut dyn salsa::Database = database;
        let salsa =
            postcard::to_stdvec(&database.as_serialize()).map_err(|error| self.encoding(&error))?;
        std::fs::write(writing.join(SALSA), &salsa)?;
        std::fs::write(writing.join(FILES), &files)?;
        std::fs::write(writing.join(CRATES), &crates)?;
        std::fs::write(
            writing.join(KEY),
            crate::run_cache::executable_fingerprint(),
        )?;
        match std::fs::remove_dir_all(&self.dir) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
        std::fs::rename(&writing, &self.dir)?;
        Ok(salsa.len() + files.len() + crates.len())
    }

    /// The saved database, or why there is none to use.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SavedState")]
    pub(super) fn restore(&self) -> Result<RootDatabase, String> {
        let key = std::fs::read_to_string(self.dir.join(KEY))
            .map_err(|error| format!("none saved ({error})"))?;
        if key != crate::run_cache::executable_fingerprint() {
            return Err("saved by another build of cordial".to_owned());
        }
        let read = |name: &str| {
            std::fs::read(self.dir.join(name))
                .map_err(|error| format!("{name} unreadable ({error})"))
        };
        let (salsa, files, crates) = (read(SALSA)?, read(FILES)?, read(CRATES)?);
        // Salsa asserts on state it does not expect; whatever happens in there, the run
        // goes on without it.
        catch_unwind(AssertUnwindSafe(|| {
            let mut database = RootDatabase::empty();
            let target: &mut dyn salsa::Database = &mut database;
            target
                .deserialize(&mut postcard::Deserializer::from_bytes(&salsa))
                .map_err(|error| format!("salsa state unreadable ({error})"))?;
            database.files().restore(
                postcard::from_bytes(&files)
                    .map_err(|error| format!("files unreadable ({error})"))?,
            );
            database.crates_map().restore(
                postcard::from_bytes(&crates)
                    .map_err(|error| format!("crates unreadable ({error})"))?,
            );
            Ok(database)
        }))
        .unwrap_or_else(|_| Err("restoring it panicked".to_owned()))
    }

    #[instrument(level = "trace", skip(self, error))]
    fn encoding(&self, error: &postcard::Error) -> CordialError {
        CordialError::perf(
            self.dir.clone(),
            format!("could not encode the state: {error}"),
        )
    }

    /// Where the state is kept.
    #[instrument(level = "trace", skip(self))]
    pub fn directory(&self) -> &Path {
        &self.dir
    }
}

impl super::SourceDatabase {
    /// Save this session's database as the next run's starting point. Once per session, at
    /// the checkpoint after the ordinary projection: the contexts' copies are made fresh by
    /// each run and are not part of what is kept. Failing to save is said and never fails
    /// the run.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn save_checkpoint(&mut self) {
        let Some(saved) = self.saved.clone() else {
            return;
        };
        if std::mem::replace(&mut self.saved_once, true) {
            return;
        }
        let started = std::time::Instant::now();
        match saved.save(&mut self.db) {
            Ok(bytes) => crate::host::event(format!(
                "analysis state: saved for the next run, {bytes} bytes in {:?}",
                started.elapsed()
            )),
            Err(error) => {
                tracing::warn!(target: "cordial::cache", error = ?error, "could not save the analysis state");
                crate::host::event("analysis state: could not be saved".to_owned());
            }
        }
    }

    /// Whether this session's database was restored from a saved state.
    #[instrument(level = "trace", skip(self))]
    pub fn restored(&self) -> bool {
        self.restored
    }
}
