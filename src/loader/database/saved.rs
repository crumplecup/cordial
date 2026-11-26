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

/// An ingredient of the Salsa state as the manifest names it. The manifest lists them in the order a
/// restore has to read them. An ingredient is a map of `entries` entries, kept as chunks in the order
/// they are read, each said by hash and length so that one cut short is found out before it is
/// decoded.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SegmentEntry {
    index: u32,
    name: String,
    /// The source file the ingredient is declared in, which says whose it is: the tool's own are under
    /// `src/`, and what the engine (rust-analyzer, Salsa) made is not.
    file: String,
    entries: u64,
    chunks: Vec<super::state_chunks::ChunkEntry>,
}

impl SegmentEntry {
    /// How many bytes the ingredient is.
    fn bytes(&self) -> u64 {
        self.chunks.iter().map(|chunk| chunk.bytes).sum()
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
    /// Whether this is the base state a project starts from when it has none of its own, and not the
    /// state the project's last run left.
    pub(super) from_base: bool,
    /// The ingredients the state held, as its manifest says them, and whether it was written with
    /// canonical stamps: what a save can take unchanged ingredients from without encoding them.
    segments: Vec<SegmentEntry>,
    canonical: bool,
}

/// What a save may take from the state the database was restored from.
#[derive(Debug, Default)]
struct Previous {
    segments: std::collections::HashMap<u32, SegmentEntry>,
    canonical: bool,
}

/// Where a database is saved between runs.
#[derive(Debug, Clone)]
pub struct SavedState {
    dir: PathBuf,
    /// A state to start from when this one is missing or cannot be used: what is common to projects
    /// (the toolchain's sysroot, built and saved once) and not what one project's last run left. It is
    /// read and never written, and the chunks this state saves that it already has are linked from it.
    base: Option<PathBuf>,
    /// For a base: the key it is made and used under, which stands for the engine and not for the build
    /// of the tool. A base holds only what the engine made (see [`crate::base_state`]), is restored
    /// into any build whose ingredients are numbered as the one that saved it numbered them, and is
    /// checked to be.
    engine: Option<String>,
    /// What the database restored from a state holds of it, for the save that follows.
    previous: std::sync::Arc<std::sync::Mutex<Option<Previous>>>,
    /// Whether a save keeps the encoding of the ingredients that have not changed. Off for the tests that
    /// compare it with encoding them all.
    reuses_unchanged: bool,
}

impl SavedState {
    /// This state, but a save of it encodes every ingredient, as it did before it kept the encoding of the ones
    /// that have not changed: for the tests that compare the two.
    #[must_use]
    pub fn without_reuse(mut self) -> Self {
        self.reuses_unchanged = false;
        self
    }

    /// Say what the state the database was restored from holds, so that a save can take what has not
    /// changed from it without encoding it.
    pub(super) fn remember(&self, restored: &mut Restored) {
        if let Ok(mut previous) = self.previous.lock() {
            *previous = Some(Previous {
                segments: std::mem::take(&mut restored.segments)
                    .into_iter()
                    .map(|entry| (entry.index, entry))
                    .collect(),
                canonical: restored.canonical,
            });
        }
    }

    /// The state kept in `dir`.
    #[instrument(level = "trace", skip(dir))]
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            base: None,
            engine: None,
            previous: Default::default(),
            reuses_unchanged: true,
        }
    }

    /// A digest of how this build numbers the ingredients the engine made (rust-analyzer's and Salsa's,
    /// not the tool's own): their index, name and source file, in index order. A base made by one build
    /// is for another that has the same digest, whatever else changed.
    #[must_use]
    pub fn engine_layout_digest() -> String {
        use sha2::{Digest, Sha256};
        let mut database = super::scan_queries::empty_database();
        let target: &mut dyn salsa::Database = &mut database;
        let mut layout = target.ingredient_layout();
        layout.retain(|(_, _, file)| !file.starts_with("src/"));
        layout.sort_unstable();
        let mut hasher = Sha256::new();
        for (index, name, file) in layout {
            hasher.update(format!("{index}:{name}:{file};").as_bytes());
        }
        hasher
            .finalize()
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// A base state kept in `dir` under `key`: saved with only what the engine made, and restored
    /// by any build that numbers the engine's ingredients as it did.
    #[must_use]
    pub fn base_at(dir: impl Into<PathBuf>, key: impl Into<String>) -> Self {
        Self {
            dir: dir.into(),
            base: None,
            engine: Some(key.into()),
            previous: Default::default(),
            reuses_unchanged: true,
        }
    }

    /// Whether this state was saved by this build of the tool, so that a run can start from it: what a
    /// project with a state of its own does not need a base for.
    #[must_use]
    pub fn is_usable(&self) -> bool {
        std::fs::read_to_string(self.dir.join(KEY))
            .is_ok_and(|key| key == crate::run_cache::executable_fingerprint())
    }

    /// This state, started from `base` (a directory a state was saved in) when it has none of its
    /// own that can be used.
    #[must_use]
    pub fn with_base(mut self, base: impl Into<PathBuf>) -> Self {
        self.base = Some(base.into());
        self
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
        // The chunks of the state being replaced are linked into this one when they are the same.
        let mut chunks = super::state_chunks::ChunkDir::new(
            self.dir.join(SALSA).join(super::state_chunks::CHUNKS),
            salsa_dir.join(super::state_chunks::CHUNKS),
        )?;
        if let Some(base) = self.base.as_ref() {
            chunks = chunks.also_linking_from(base.join(SALSA).join(super::state_chunks::CHUNKS));
        }
        let previous = self.previous.lock().ok().and_then(|mut held| held.take());
        let mut taken = 0_u64;
        // How the state restored from cut each ingredient, for the ones that can be left to it where
        // nothing in a chunk changed: those it has every chunk of, and that say how they were cut.
        if self.reuses_unchanged
            && let Some(previous) = previous.as_ref().filter(|previous| previous.canonical)
        {
            let plan: Vec<(u32, Vec<salsa::PlannedChunk>)> = previous
                .segments
                .values()
                .filter(|entry| {
                    !entry.chunks.is_empty()
                        && entry.chunks.iter().all(|chunk| chunk.entries > 0)
                        && chunks.has_all(&entry.chunks)
                })
                .map(|entry| {
                    (
                        entry.index,
                        entry
                            .chunks
                            .iter()
                            .map(|chunk| salsa::PlannedChunk {
                                first_key: chunk.first_key.clone(),
                                entries: chunk.entries,
                                keys: chunk.keys,
                            })
                            .collect(),
                    )
                })
                .collect();
            database.set_chunk_plan(plan);
        }
        database.save_segments(
            |header| {
                header_len = self.write_encoded(&salsa_dir.join(SALSA_HEADER), header)?;
                Ok::<(), CordialError>(())
            },
            |segment, part| {
                // A base holds what the engine made, and not the tool's own memos, which are of the
                // build that made them and mean nothing to a project's.
                if self.engine.is_some() && segment.file.starts_with("src/") {
                    return Ok(());
                }
                let started = std::time::Instant::now();
                // What changed nothing since the state was restored is that state's: its chunks are
                // linked, and the ingredient is not encoded or hashed again.
                let reused = if self.reuses_unchanged
                    && segment.unchanged
                    && segment.canonical
                    && previous.as_ref().is_some_and(|previous| previous.canonical)
                {
                    previous
                        .as_ref()
                        .and_then(|previous| previous.segments.get(&segment.index))
                        .filter(|entry| entry.name == segment.name && entry.file == segment.file)
                        .filter(|entry| chunks.reuse(&entry.chunks))
                        .cloned()
                } else {
                    None
                };
                let (entry, was_reused) = match reused {
                    Some(entry) => (entry, true),
                    None => {
                        let before = previous
                            .as_ref()
                            .and_then(|previous| previous.segments.get(&segment.index))
                            .filter(|entry| {
                                entry.name == segment.name && entry.file == segment.file
                            })
                            .map_or(&[][..], |entry| entry.chunks.as_slice());
                        let written = super::state_chunks::write_chunked(part, &mut chunks, before)
                            .map_err(|error| self.chunking(segment.name, &error))?;
                        (
                            SegmentEntry {
                                index: segment.index,
                                name: segment.name.to_owned(),
                                file: segment.file.to_owned(),
                                entries: written.entries,
                                chunks: written.chunks,
                            },
                            false,
                        )
                    }
                };
                let micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
                let after = crate::process_memory::sample();
                tracing::debug!(
                    target: "salsa::state_size",
                    ingredient = segment.name,
                    index = segment.index,
                    bytes = entry.bytes(),
                    entries = entry.entries,
                    chunks = entry.chunks.len() as u64,
                    reused = was_reused,
                    micros,
                    resident_kib = after.map_or(0, |sample| sample.resident_kib()),
                    peak_kib = after.map_or(0, |sample| sample.peak_kib()),
                    "an ingredient's share of the saved state"
                );
                taken += u64::from(was_reused);
                manifest.push(entry);
                Ok(())
            },
        )?;
        crate::host::event(format!(
            "analysis state: {taken} of {} ingredients unchanged since the last state, not encoded again",
            manifest.len()
        ));
        let manifest_len = self.write_encoded(&salsa_dir.join(SALSA_MANIFEST), &manifest)?;
        let salsa_len = header_len
            + manifest_len
            + manifest
                .iter()
                .map(|entry| usize::try_from(entry.bytes()).unwrap_or(usize::MAX))
                .sum::<usize>();
        tracing::debug!(
            target: "cordial::cache",
            step = "chunks",
            chunks_written = chunks.written,
            chunks_linked = chunks.linked,
            chunks_repeated = chunks.repeated,
            chunk_bytes_written = chunks.bytes_written,
            chunk_bytes_linked = chunks.bytes_linked,
            chunks_taken = chunks.taken,
            "the chunks of the saved state"
        );
        crate::host::event(format!(
            "analysis state: {} chunks written ({} bytes), {} linked from the last state ({} bytes), {} of them taken without encoding their entries",
            chunks.written, chunks.bytes_written, chunks.linked, chunks.bytes_linked, chunks.taken
        ));
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
            self.engine
                .clone()
                .unwrap_or_else(crate::run_cache::executable_fingerprint),
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
        let own = match self.restore_own() {
            Ok(restored) => return Ok(restored),
            Err(reason) => reason,
        };
        let Some(base) = self.base.as_ref() else {
            return Err(own);
        };
        let started = std::time::Instant::now();
        // A base is named for its key.
        let engine = base
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        match (Self {
            dir: base.clone(),
            base: None,
            engine,
            previous: Default::default(),
            reuses_unchanged: true,
        })
        .restore_own()
        {
            Ok(mut restored) => {
                tracing::info!(
                    target: "cordial::cache",
                    own = %own,
                    micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
                    "no state of the project's own to use: started from the base state"
                );
                restored.from_base = true;
                Ok(restored)
            }
            Err(from_base) => Err(format!("{own}; and the base state: {from_base}")),
        }
    }

    /// The state this one saved last, if it can be used.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SavedState")]
    fn restore_own(&self) -> Result<Restored, String> {
        let key = std::fs::read_to_string(self.dir.join(KEY))
            .map_err(|error| format!("none saved ({error})"))?;
        let expected = self
            .engine
            .clone()
            .unwrap_or_else(crate::run_cache::executable_fingerprint);
        if key != expected {
            return Err(if self.engine.is_some() {
                "a base state of another engine".to_owned()
            } else {
                "saved by another build of cordial".to_owned()
            });
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
            let (segments, canonical) = self.decode_salsa(&mut database)?;
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
                from_base: false,
                segments,
                canonical,
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

    /// Whether this build numbers the ingredients a base saved as the build that saved it did: each has
    /// the index, the name and the source file it had. A base made by another build of the tool is
    /// restored only when this holds; one that does not is a base for no one, and a run starts cold.
    fn verify_layout(
        manifest: &[SegmentEntry],
        database: &mut dyn salsa::Database,
    ) -> Result<(), String> {
        let layout: std::collections::HashMap<u32, (&'static str, &'static str)> = database
            .ingredient_layout()
            .into_iter()
            .map(|(index, name, file)| (index, (name, file)))
            .collect();
        for entry in manifest {
            match layout.get(&entry.index) {
                Some((name, file)) if *name == entry.name && *file == entry.file => {}
                Some((name, file)) => {
                    return Err(format!(
                        "the base saved ingredient {} ({}, {}) where this build has {name} ({file})",
                        entry.index, entry.name, entry.file
                    ));
                }
                None => {
                    return Err(format!(
                        "the base saved ingredient {} ({}) which this build does not have",
                        entry.index, entry.name
                    ));
                }
            }
        }
        Ok(())
    }

    /// Rebuild the Salsa memos from their saved segments, in the order the manifest gives. The events
    /// give each ingredient's bytes and time and then the whole, so the rate shows whether decoding or
    /// something else is what a restore waits for.
    #[instrument(level = "debug", skip_all)]
    #[hotpath::measure(impl_type = "SavedState")]
    fn decode_salsa(
        &self,
        database: &mut RootDatabase,
    ) -> Result<(Vec<SegmentEntry>, bool), String> {
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
        if self.engine.is_some() {
            Self::verify_layout(&manifest, target)?;
        }
        let mut bytes = 0_u64;
        let chunk_dir = dir.join(super::state_chunks::CHUNKS);
        for entry in &manifest {
            let reader =
                super::state_chunks::chunked_reader(&chunk_dir, entry.entries, &entry.chunks)
                    .map_err(|error| format!("salsa {} unreadable ({error})", entry.name))?;
            target
                .restore_segment(
                    &header,
                    entry.index,
                    &mut rmp_serde::Deserializer::new(reader),
                )
                .map_err(|error| format!("salsa {} unreadable ({error})", entry.name))?;
            bytes += entry.bytes();
        }
        let canonical = header.is_canonical();
        target.finish_segments(header);
        tracing::debug!(
            target: "cordial::cache",
            bytes,
            segments = manifest.len(),
            edges = salsa::edges_expanded() - expanded_before,
            micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
            "decoded the saved Salsa memos"
        );
        Ok((manifest, canonical))
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
    fn chunking(&self, ingredient: &str, error: &super::state_chunks::ChunkError) -> CordialError {
        CordialError::perf(
            self.dir.clone(),
            format!("could not save {ingredient} as chunks: {error}"),
        )
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
        {
            let _fingerprints = crate::Segment::begin("fingerprints");
            self.ask_fingerprints();
        }
        crate::process_memory::log("save: fingerprints asked");
        let verifying = std::time::Instant::now();
        let (queries_before, (memos_before, edges_before), counts_before) = (
            crate::LiveStatus::global().executed(),
            salsa::deep_verifications(),
            crate::LiveStatus::global().counts(),
        );
        tracing::info!(
            target: "cordial::cache",
            step = "verifying",
            threads = std::thread::available_parallelism().map_or(1, usize::from) as u64,
            "persisted memos are about to be verified before the save"
        );
        {
            let _verify = crate::Segment::begin("verify_persisted");
            self.verify_persisted_in_parallel();
        }
        let (memos_after, edges_after) = salsa::deep_verifications();
        crate::process_memory::log("save: persisted memos verified");
        tracing::info!(
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
        let saved_state = {
            let _write = crate::Segment::begin("write");
            saved.save(&mut self.db, &self.vfs, &self.context_crates)
        };
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
