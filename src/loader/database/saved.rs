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
use ra_ap_vfs::{Vfs, VfsPath};
use tracing::instrument;

use crate::{CordialError, CordialResult};

const KEY: &str = "key";
const SALSA: &str = "salsa";
const FILES: &str = "files";
const CRATES: &str = "crates";
const PATHS: &str = "paths";

/// What a saved state restores to: the database, and the paths of the VFS it was saved with,
/// in id order. Salsa's inputs are keyed by file id, so the next load must hand out the same
/// ids to the same paths: it seeds its VFS with these.
pub(super) struct Restored {
    pub(super) database: RootDatabase,
    pub(super) paths: Vec<VfsPath>,
}

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
    pub(super) fn save(&self, database: &mut RootDatabase, vfs: &Vfs) -> CordialResult<usize> {
        let name = self
            .dir
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let writing = self
            .dir
            .with_file_name(format!("{name}.writing-{}", std::process::id()));
        self.sweep_abandoned(&name, &writing);
        let _ = std::fs::remove_dir_all(&writing);
        std::fs::create_dir_all(&writing)?;
        let paths: Vec<&VfsPath> = vfs.interned_paths().collect();
        let paths = rmp_serde::to_vec(&paths).map_err(|error| self.encoding(&error))?;
        let files =
            rmp_serde::to_vec(&database.files().dump()).map_err(|error| self.encoding(&error))?;
        let crates = rmp_serde::to_vec(&database.crates_map().dump())
            .map_err(|error| self.encoding(&error))?;
        let database: &mut dyn salsa::Database = database;
        let salsa =
            rmp_serde::to_vec(&database.as_serialize()).map_err(|error| self.encoding(&error))?;
        std::fs::write(writing.join(SALSA), &salsa)?;
        std::fs::write(writing.join(FILES), &files)?;
        std::fs::write(writing.join(CRATES), &crates)?;
        std::fs::write(writing.join(PATHS), &paths)?;
        std::fs::write(
            writing.join(KEY),
            crate::run_cache::executable_fingerprint(),
        )?;
        match std::fs::remove_dir_all(&self.dir) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
        std::fs::rename(&writing, &self.dir)?;
        Ok(salsa.len() + files.len() + crates.len() + paths.len())
    }

    /// The saved database, or why there is none to use.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SavedState")]
    pub(super) fn restore(&self) -> Result<Restored, String> {
        let key = std::fs::read_to_string(self.dir.join(KEY))
            .map_err(|error| format!("none saved ({error})"))?;
        if key != crate::run_cache::executable_fingerprint() {
            return Err("saved by another build of cordial".to_owned());
        }
        let read = |name: &str| {
            std::fs::read(self.dir.join(name))
                .map_err(|error| format!("{name} unreadable ({error})"))
        };
        let (salsa, files, crates, paths) =
            (read(SALSA)?, read(FILES)?, read(CRATES)?, read(PATHS)?);
        // Salsa asserts on state it does not expect; whatever happens in there, the run
        // goes on without it.
        catch_unwind(AssertUnwindSafe(|| {
            let mut database = RootDatabase::empty();
            let target: &mut dyn salsa::Database = &mut database;
            target
                .deserialize(&mut rmp_serde::Deserializer::from_read_ref(
                    salsa.as_slice(),
                ))
                .map_err(|error| format!("salsa state unreadable ({error})"))?;
            database.files().restore(
                rmp_serde::from_slice(&files)
                    .map_err(|error| format!("files unreadable ({error})"))?,
            );
            database.crates_map().restore(
                rmp_serde::from_slice(&crates)
                    .map_err(|error| format!("crates unreadable ({error})"))?,
            );
            let paths = rmp_serde::from_slice(&paths)
                .map_err(|error| format!("paths unreadable ({error})"))?;
            Ok(Restored { database, paths })
        }))
        .unwrap_or_else(|_| Err("restoring it panicked".to_owned()))
    }

    /// Remove what a save that never finished left beside the slot: a directory written
    /// to and not moved into place, by a process that died. Another process's directory is
    /// only removed once it is old enough that its writer cannot still be at it.
    #[instrument(level = "debug", skip(self, ours))]
    #[hotpath::measure(impl_type = "SavedState")]
    fn sweep_abandoned(&self, name: &str, ours: &Path) {
        const ABANDONED_AFTER: std::time::Duration = std::time::Duration::from_secs(10 * 60);
        let (Some(parent), prefix) = (self.dir.parent(), format!("{name}.writing-")) else {
            return;
        };
        let Ok(entries) = std::fs::read_dir(parent) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let old = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age > ABANDONED_AFTER);
            if path != ours && old && entry.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }

    #[instrument(level = "trace", skip(self, error))]
    fn encoding(&self, error: &rmp_serde::encode::Error) -> CordialError {
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
        match saved.save(&mut self.db, &self.vfs) {
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

    /// Call once the persisted queries that other persisted queries depend on.
    ///
    /// Salsa sets up a tracked function the first time it is called in a process, and
    /// verifying a restored memo checks the memos it depends on, which needs the callee set
    /// up. A persisted query that depends on another persisted query would otherwise panic
    /// when it is the first to be asked for. Those dependencies are `file_facts` and
    /// `persisted_evidence` (read by `file_items` and the tracing inventory); asking each of
    /// them one harmless question sets them up. The saved-state test that a restored run computes
    /// no per-file query panics here when a persisted query gains a persisted dependency that
    /// is not listed.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn warm_persisted_dependencies(&self) {
        ra_ap_hir::attach_db(&self.db, || {
            let nothing = super::item_key::ItemKey::new(&self.db, String::new());
            let _ = super::queries::persisted_evidence(&self.db, nothing);
            // A file of the workspace itself, which the projection asks about anyway.
            let roots: Vec<PathBuf> = self
                .projection_context()
                .map(|context| {
                    context
                        .packages()
                        .iter()
                        .map(|package| package.root().clone())
                        .collect()
                })
                .unwrap_or_default();
            let real = self.vfs.iter().find_map(|(file, path)| {
                let path: &Path = path.as_path()?.as_ref();
                (path.extension().is_some_and(|ext| ext == "rs")
                    && roots.iter().any(|root| path.starts_with(root)))
                .then(|| (file, path.to_path_buf()))
            });
            if let Some((file, path)) = real {
                let key = super::queries::FileKey::new(&self.db, file, path);
                let _ = super::queries::file_facts(&self.db, key);
            }
        });
    }
}
