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

use super::edition::crate_name;
use ra_ap_ide_db::RootDatabase;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use ra_ap_vfs::{Vfs, VfsPath};
use tracing::instrument;

use crate::{CordialError, CordialResult};

const KEY: &str = "key";
/// The directory of the Salsa state: a header, a manifest and a file for each ingredient.
const SALSA: &str = "salsa";
const SALSA_HEADER: &str = "header";
const SALSA_MANIFEST: &str = "manifest";
const FILES: &str = "files";
const CRATES: &str = "crates";
const PATHS: &str = "paths";
const CONTEXTS: &str = "contexts";

/// How much of the encoded state is held before it is written to the file.
const WRITE_BUFFER_BYTES: usize = 1 << 20;

/// How much of the saved Salsa state is buffered while its memos are restored.
const READ_BUFFER_BYTES: usize = 64 * 1024;

/// An ingredient of the Salsa state as the manifest names it. The manifest lists them in the order a
/// restore has to read them, and says how long each is so that a file cut short is found out before
/// it is decoded.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SegmentEntry {
    index: u32,
    name: String,
    bytes: u64,
}

impl SegmentEntry {
    /// The file the ingredient is in, within the Salsa directory.
    fn file(&self) -> String {
        format!("{}.seg", self.index)
    }
}

/// What a saved state restores to: the database, and the paths of the VFS it was saved with,
/// in id order. Salsa's inputs are keyed by file id, so the next load must hand out the same
/// ids to the same paths: it seeds its VFS with these.
pub(super) struct Restored {
    pub(super) database: RootDatabase,
    pub(super) paths: Vec<VfsPath>,
    /// The cfg contexts the saved run had added, as that run left them.
    pub(super) contexts: Vec<super::ContextCrates>,
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

    /// Where this run's spilled values go: a file named for the process, in a directory beside the
    /// state (in the run cache, which is on the disk, and not in `/tmp`, which may be memory).
    pub(super) fn spill_path(&self) -> PathBuf {
        self.dir
            .with_file_name("analysis-spill")
            .join(format!("{}.redb", std::process::id()))
    }

    /// Save `database`, replacing what was saved. Written beside the state and moved into
    /// place, with the key last, so a reader never sees half of one. Returns the bytes
    /// written.
    #[instrument(level = "debug", skip(self, database), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SavedState")]
    pub(super) fn save(
        &self,
        database: &mut RootDatabase,
        vfs: &Vfs,
        contexts: &[super::ContextCrates],
    ) -> CordialResult<usize> {
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
        crate::host::event("analysis state: writing".to_owned());
        let paths: Vec<&VfsPath> = vfs.interned_paths().collect();
        let paths = rmp_serde::to_vec(&paths).map_err(|error| self.encoding(&error))?;
        let files =
            rmp_serde::to_vec(&database.files().dump()).map_err(|error| self.encoding(&error))?;
        let crates = rmp_serde::to_vec(&database.crates_map().dump())
            .map_err(|error| self.encoding(&error))?;
        let order = ra_ap_ide_db::base_db::all_crates(&*database);
        let contexts: Vec<_> = contexts
            .iter()
            .filter_map(|context| context.to_saved(&order))
            .collect();
        let contexts = rmp_serde::to_vec(&contexts).map_err(|error| self.encoding(&error))?;
        super::cold_texts::attach_library_sources(database, vfs);
        let database: &mut dyn salsa::Database = database;
        crate::host::event("analysis state: encoding the memos".to_owned());
        crate::process_memory::log("save: encoding begun");
        let encoding = std::time::Instant::now();
        // Straight to the file: the encoded state held in memory first, and grown as it is written,
        // is a copy or two of the state on top of what the run holds, at the moment a run that is
        // saving because its memory is near the budget can least afford it.
        let salsa_dir = writing.join(SALSA);
        std::fs::create_dir_all(&salsa_dir)?;
        let mut manifest = Vec::new();
        let mut header_len = 0_usize;
        database.save_segments(
            |header| {
                header_len = self.write_encoded(&salsa_dir.join(SALSA_HEADER), header)?;
                Ok::<(), CordialError>(())
            },
            |segment, part| {
                let entry = SegmentEntry {
                    index: segment.index,
                    name: segment.name.to_owned(),
                    bytes: 0,
                };
                let bytes = self.write_encoded(&salsa_dir.join(entry.file()), part)?;
                tracing::debug!(
                    target: "salsa::state_size",
                    ingredient = segment.name,
                    index = segment.index,
                    bytes,
                    "an ingredient's share of the saved state"
                );
                manifest.push(SegmentEntry {
                    bytes: u64::try_from(bytes).unwrap_or(u64::MAX),
                    ..entry
                });
                Ok(())
            },
        )?;
        let manifest_len = self.write_encoded(&salsa_dir.join(SALSA_MANIFEST), &manifest)?;
        let salsa_len = header_len
            + manifest_len
            + manifest
                .iter()
                .map(|entry| usize::try_from(entry.bytes).unwrap_or(usize::MAX))
                .sum::<usize>();
        tracing::debug!(
            target: "cordial::cache",
            step = "encoded",
            bytes = salsa_len,
            micros = u64::try_from(encoding.elapsed().as_micros()).unwrap_or(u64::MAX),
            "the memos encoded"
        );
        crate::host::event(format!(
            "analysis state: memos encoded, {} bytes in {:?}",
            salsa_len,
            encoding.elapsed()
        ));
        std::fs::write(writing.join(FILES), &files)?;
        std::fs::write(writing.join(CRATES), &crates)?;
        std::fs::write(writing.join(PATHS), &paths)?;
        std::fs::write(writing.join(CONTEXTS), &contexts)?;
        std::fs::write(
            writing.join(KEY),
            crate::run_cache::executable_fingerprint(),
        )?;
        match std::fs::remove_dir_all(&self.dir) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
        std::fs::rename(&writing, &self.dir)?;
        Ok(salsa_len + files.len() + crates.len() + paths.len() + contexts.len())
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
        let (files, crates, paths, contexts) = (
            self.read_part(FILES)?,
            self.read_part(CRATES)?,
            self.read_part(PATHS)?,
            self.read_part(CONTEXTS)?,
        );
        // Salsa asserts on state it does not expect; whatever happens in there, the run
        // goes on without it.
        catch_unwind(AssertUnwindSafe(|| {
            let mut database = RootDatabase::empty();
            self.decode_salsa(&mut database)?;
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
            let saved: Vec<super::context_crates::SavedContext> = rmp_serde::from_slice(&contexts)
                .map_err(|error| format!("contexts unreadable ({error})"))?;
            let order = ra_ap_ide_db::base_db::all_crates(&database);
            let contexts = saved
                .into_iter()
                .map(|context| super::ContextCrates::from_saved(context, &order))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| "a saved context names a crate that is not there".to_owned())?;
            ra_ap_hir_ty::query_ownership::register(&mut database);
            super::scan_queries::register_query_ownership(&mut database);
            let roots: Vec<_> = contexts
                .iter()
                .flat_map(super::ContextCrates::shadows)
                .map(|krate| salsa::plumbing::AsId::as_id(&krate))
                .collect();
            salsa::activate_owner_roots(&mut database, &roots);
            Ok(Restored {
                database,
                paths,
                contexts,
            })
        }))
        .unwrap_or_else(|_| Err("restoring it panicked".to_owned()))
    }

    /// Encode `part` straight to `path`, and say how many bytes that was.
    #[instrument(level = "trace", skip(self, part))]
    fn write_encoded<T: serde::Serialize + ?Sized>(
        &self,
        path: &Path,
        part: &T,
    ) -> CordialResult<usize> {
        let mut out =
            std::io::BufWriter::with_capacity(WRITE_BUFFER_BYTES, std::fs::File::create(path)?);
        rmp_serde::encode::write(&mut out, part).map_err(|error| self.encoding(&error))?;
        std::io::Write::flush(&mut out)?;
        drop(out);
        Ok(usize::try_from(std::fs::metadata(path)?.len()).unwrap_or(usize::MAX))
    }

    /// One part of the saved state, as bytes. The event says how fast the disk gave it up.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SavedState")]
    fn read_part(&self, name: &str) -> Result<Vec<u8>, String> {
        let started = std::time::Instant::now();
        let bytes = std::fs::read(self.dir.join(name))
            .map_err(|error| format!("{name} unreadable ({error})"))?;
        tracing::debug!(
            target: "cordial::cache",
            part = name,
            bytes = bytes.len(),
            micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
            "read a part of the saved state"
        );
        Ok(bytes)
    }

    /// Rebuild the Salsa memos from their saved segments, in the order the manifest gives. The events
    /// give each ingredient's bytes and time and then the whole, so the rate shows whether decoding or
    /// something else is what a restore waits for.
    #[instrument(level = "debug", skip_all)]
    #[hotpath::measure(impl_type = "SavedState")]
    fn decode_salsa(&self, database: &mut RootDatabase) -> Result<(), String> {
        let started = std::time::Instant::now();
        let dir = self.dir.join(SALSA);
        let read = |name: &str| {
            std::fs::read(dir.join(name))
                .map_err(|error| format!("salsa {name} unreadable ({error})"))
        };
        let manifest: Vec<SegmentEntry> = rmp_serde::from_slice(&read(SALSA_MANIFEST)?)
            .map_err(|error| format!("salsa {SALSA_MANIFEST} unreadable ({error})"))?;
        let header: salsa::SegmentHeader = rmp_serde::from_slice(&read(SALSA_HEADER)?)
            .map_err(|error| format!("salsa {SALSA_HEADER} unreadable ({error})"))?;
        let expanded_before = salsa::edges_expanded();
        let target: &mut dyn salsa::Database = database;
        let mut bytes = 0_u64;
        for entry in &manifest {
            let file = std::fs::File::open(dir.join(entry.file()))
                .map_err(|error| format!("salsa {} unreadable ({error})", entry.name))?;
            let length = file
                .metadata()
                .map_err(|error| format!("salsa {} unreadable ({error})", entry.name))?
                .len();
            if length != entry.bytes {
                return Err(format!(
                    "salsa {} is {length} bytes where the manifest says {}",
                    entry.name, entry.bytes
                ));
            }
            let reader = std::io::BufReader::with_capacity(READ_BUFFER_BYTES, file);
            target
                .restore_segment(
                    &header,
                    entry.index,
                    &mut rmp_serde::Deserializer::new(reader),
                )
                .map_err(|error| format!("salsa {} unreadable ({error})", entry.name))?;
            bytes += length;
        }
        target.finish_segments(header);
        tracing::debug!(
            target: "cordial::cache",
            bytes,
            segments = manifest.len(),
            edges = salsa::edges_expanded() - expanded_before,
            micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
            "decoded the saved Salsa memos"
        );
        Ok(())
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

/// How many parts each thread's share of the persisted memos is cut into.
const PARTS_PER_THREAD: usize = 8;

/// Stack for a verifying thread, as a priming worker has.
const VERIFY_STACK_BYTES: usize = 64 * 1024 * 1024;

impl super::SourceDatabase {
    /// Save this session's database as the next run's starting point: once per session, at the
    /// end of the first projection, with the contexts' crates and file copies, which the next
    /// run takes up again. A session restored from a state that has computed no persisted query
    /// since saves nothing: what it would write is what is there. Failing to save is said and
    /// never fails the run.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn save_state(&mut self) {
        let Some(saved) = self.saved.clone() else {
            return;
        };
        if crate::result_store::active() {
            // What a store gave this run has no record of what it was computed from, so a state
            // saved from it would be taken for good by a run after, whatever had changed.
            crate::host::event("analysis state: not saved by a run in processes".to_owned());
            return;
        }
        if std::mem::replace(&mut self.saved_once, true) {
            return;
        }
        if self.restored && salsa::persisted_executions() == self.persisted_at_load {
            crate::host::event("analysis state: nothing new to save".to_owned());
            return;
        }
        // A memo is saved with what the queries it reads depend on now, so it must be valid as of
        // now: one a query it reads has run again under since was last verified before that, and
        // would be saved with dependencies it did not have then and be found out of date.
        // What read a definition map is saved as depending on its fingerprint, which has to have been
        // asked for.
        crate::process_memory::log("save begun");
        self.ask_fingerprints();
        crate::process_memory::log("save: fingerprints asked");
        let verifying = std::time::Instant::now();
        let (queries_before, (memos_before, edges_before), counts_before) = (
            crate::LiveStatus::global().executed(),
            salsa::deep_verifications(),
            crate::LiveStatus::global().counts(),
        );
        self.verify_persisted_in_parallel();
        let (memos_after, edges_after) = salsa::deep_verifications();
        crate::process_memory::log("save: persisted memos verified");
        tracing::debug!(
            target: "cordial::cache",
            step = "verified",
            micros = u64::try_from(verifying.elapsed().as_micros()).unwrap_or(u64::MAX),
            queries = crate::LiveStatus::global().executed().saturating_sub(queries_before),
            memos = memos_after.saturating_sub(memos_before),
            edges = edges_after.saturating_sub(edges_before),
            top = %crate::LiveStatus::global().top_since(&counts_before, 40),
            "persisted memos verified before the save"
        );
        self.reporter.say("Saving the analysis state");
        let started = std::time::Instant::now();
        let saved_state = saved.save(&mut self.db, &self.vfs, &self.context_crates);
        crate::process_memory::log("save: state written");
        match saved_state {
            Ok(bytes) => {
                tracing::debug!(
                    target: "cordial::cache",
                    step = "saved",
                    bytes,
                    micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
                    "the analysis state saved"
                );
                crate::host::event(format!(
                    "analysis state: saved for the next run, {bytes} bytes in {:?}",
                    started.elapsed()
                ));
            }
            Err(error) => {
                tracing::warn!(target: "cordial::cache", error = ?error, "could not save the analysis state");
                crate::host::event("analysis state: could not be saved".to_owned());
            }
        }
    }

    /// Say, as events, what the numbers in the database's events are: each file text's number and
    /// path, each crate's number and root file (a context's copies marked). Without it an id in
    /// a `salsa::stale` or `salsa::write` event is only a number.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn log_tables(&self) {
        use salsa::plumbing::AsId;
        if !tracing::enabled!(target: "cordial::tables", tracing::Level::DEBUG) {
            return;
        }
        for (file, key) in self.db.files().dump().texts() {
            tracing::debug!(
                target: "cordial::tables",
                table = "file_text",
                key,
                path = %self.vfs.file_path(file),
                "an input"
            );
        }
        let shadows = self.shadow_crates();
        for krate in ra_ap_ide_db::base_db::all_crates(&self.db).iter() {
            tracing::debug!(
                target: "cordial::tables",
                table = "crate",
                key = krate.as_id().as_bits(),
                hex = format_args!("{:#x}", krate.as_id().as_bits()),
                name = %crate_name(&self.db, *krate),
                root = %self.vfs.file_path(krate.data(&self.db).root_file_id),
                context_copy = shadows.contains(krate),
                "an input"
            );
        }
    }

    /// Verify every persisted memo at the current revision, a part of them on each of several
    /// threads. Verifying one is a walk through what it reads, which the memos share a great deal of
    /// and which a thread only reads, so the parts go as queries do on several handles of one
    /// database. More parts than threads, so a thread that finishes early takes another: some
    /// parts walk through far more than others.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn verify_persisted_in_parallel(&self) {
        use rayon::iter::{IndexedParallelIterator, IntoParallelIterator, ParallelIterator};
        let threads = std::thread::available_parallelism().map_or(1, usize::from);
        let parts = threads * PARTS_PER_THREAD;
        // Inferring a body recurses deeply, as it does for a priming worker.
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .stack_size(VERIFY_STACK_BYTES)
            .thread_name(|index| format!("cordial-verify-{index}"))
            .build();
        let Ok(pool) = pool else {
            tracing::warn!("could not start the verifying threads: verifying on this one");
            ra_ap_hir::attach_db(&self.db, || {
                <dyn salsa::Database>::verify_persisted_memos(&self.db);
            });
            return;
        };
        let dispatch = tracing::dispatcher::get_default(Clone::clone);
        // First what flattening a memo's dependencies is enough to verify, a share of the
        // ingredients on each thread: it runs no query, so the threads only read.
        let by_closure: Vec<_> = (0..threads).map(|_| self.db.clone()).collect();
        pool.install(|| {
            by_closure
                .into_par_iter()
                .enumerate()
                .for_each(|(part, db)| {
                    tracing::dispatcher::with_default(&dispatch, || {
                        ra_ap_hir::attach_db(&db, || {
                            verify_closure_part(&db, part, threads);
                        });
                    });
                });
        });
        // Then the rest, a part of the memos on each thread, by walking what they read. A handle
        // of its own for each part, and the subscriber of this thread for the others.
        let handles: Vec<_> = (0..parts).map(|_| self.db.clone()).collect();
        pool.install(|| {
            handles.into_par_iter().enumerate().for_each(|(part, db)| {
                tracing::dispatcher::with_default(&dispatch, || {
                    ra_ap_hir::attach_db(&db, || {
                        verify_query_part(&db, part, parts);
                    });
                });
            });
        });
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
    pub(super) fn warm_persisted_dependencies(&mut self, paths: &[PathBuf]) {
        let before = salsa::persisted_executions();
        ra_ap_hir::attach_db(&self.db, || {
            let nothing = super::item_key::ItemKey::new(&self.db, String::new());
            let _ = super::queries::persisted_evidence(&self.db, nothing);
        });
        // What asking about nothing computed is not new to what was saved: it is the
        // question's own answer, and the same in every run.
        self.persisted_at_load += salsa::persisted_executions() - before;
        // A file the projection is about to ask about anyway, so that what this asks is
        // answered by the memo the last run saved and nothing is computed for it.
        ra_ap_hir::attach_db(&self.db, || {
            if let Some(path) = paths.first()
                && let Ok(file) = self.file_id(path)
            {
                let key = super::queries::FileKey::new(&self.db, file, path.clone());
                let _ = super::queries::file_facts(&self.db, key);
            }
        });
    }
}

// Named worker boundaries retain dependency attribution on the worker's thread.
#[hotpath::measure]
fn verify_closure_part(db: &RootDatabase, part: usize, parts: usize) {
    // Attached, so that what runs again is named in Salsa's events and not a bare key.
    salsa::attach(db, || {
        <dyn salsa::Database>::verify_persisted_memos_by_closure_part(db, part, parts);
    });
}

#[hotpath::measure]
fn verify_query_part(db: &RootDatabase, part: usize, parts: usize) {
    salsa::attach(db, || {
        <dyn salsa::Database>::verify_persisted_memos_part(db, part, parts);
    });
}
