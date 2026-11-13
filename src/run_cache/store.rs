//! Cached analysis artifacts, one directory per key.
//!
//! An entry is `{store}/cache/run/{digest}/` holding one compressed IR per
//! target, the workspace path-inclusion facts, and the inputs that produced
//! them. Each artifact is written atomically and is valid on its own for its
//! key, so a run that needs one the entry lacks computes just that one and adds
//! it. Only the entry for the current key is kept.

use super::key::Inputs;
use crate::{CordialResult, CrateIr, StoreLayout};
use serde::{Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::instrument;

const INPUTS_FILE: &str = "inputs.json";
const ZSTD_LEVEL: i32 = 3;

/// The cache entry for one key.
#[derive(Debug)]
pub(crate) struct RunCache {
    /// `{store}/cache/run`, holding one directory per key ever written.
    root: PathBuf,
    directory: PathBuf,
    inputs: Inputs,
    pruned: AtomicBool,
    /// Writes of crate IR that are still running on their own threads.
    pending: std::sync::Mutex<Vec<std::thread::JoinHandle<CordialResult<()>>>>,
}

impl RunCache {
    /// The entry for `inputs` in `store`; nothing is created until a write.
    #[instrument(level = "debug", skip(store, inputs))]
    #[hotpath::measure]
    pub(crate) fn open(store: &StoreLayout, inputs: Inputs) -> Self {
        let root = store.cache_dir().join("run");
        Self {
            directory: root.join(inputs.digest()),
            root,
            inputs,
            pruned: AtomicBool::new(false),
            pending: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// A target's cached IR, if this entry has it.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "RunCache")]
    pub(crate) fn read_crate_ir(&self, crate_name: &str) -> CordialResult<Option<CrateIr>> {
        let path = self.crate_path(crate_name);
        if !path.is_file() {
            return Ok(None);
        }
        CrateIr::read_cache(&path).map(Some)
    }

    /// Record a target's IR in this entry. The snapshot is taken now and written
    /// on another thread, so the run goes on to its later stages while the file
    /// is encoded; [`Self::finish_writes`] waits for it.
    #[instrument(level = "debug", skip(self, ir), err(level = "warn"))]
    #[hotpath::measure(impl_type = "RunCache")]
    pub(crate) fn write_crate_ir(&self, crate_name: &str, ir: &CrateIr) -> CordialResult<()> {
        self.begin_write()?;
        let path = self.crate_path(crate_name);
        let partial = partial_path(&path);
        let snapshot = ir.snapshot()?;
        let dispatch = tracing::dispatcher::get_default(Clone::clone);
        let writer = std::thread::Builder::new()
            .name("cordial-ir-cache-writer".to_owned())
            .spawn(move || {
                tracing::dispatcher::with_default(&dispatch, || {
                    snapshot.write_cache(&partial)?;
                    std::fs::rename(&partial, &path)?;
                    Ok(())
                })
            })?;
        locked(&self.pending).push(writer);
        Ok(())
    }

    /// Wait for the IR writes this entry started. A failed write leaves no file
    /// (a partial one is never renamed into place), so the next run analyses
    /// again; it is logged, not an error of this run.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "RunCache")]
    pub(crate) fn finish_writes(&self) {
        let writers = std::mem::take(&mut *locked(&self.pending));
        for writer in writers {
            match writer.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::warn!(target: "cordial::cache", error = ?error, "could not record the analysis");
                }
                Err(_) => {
                    tracing::warn!(target: "cordial::cache", "the cache writer panicked");
                }
            }
        }
    }

    /// The workspace path-inclusion facts, if this entry has them.
    #[cfg(feature = "derives")]
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure]
    pub(crate) fn read_path_inclusions(
        &self,
    ) -> CordialResult<Option<crate::ir::PathInclusionFacts>> {
        read_json(&self.directory.join("path_inclusions.json.zst"))
    }

    /// Record the workspace path-inclusion facts in this entry.
    #[cfg(feature = "derives")]
    #[instrument(level = "debug", skip(self, facts), err(level = "warn"))]
    #[hotpath::measure]
    pub(crate) fn write_path_inclusions(
        &self,
        facts: &crate::ir::PathInclusionFacts,
    ) -> CordialResult<()> {
        self.begin_write()?;
        write_json(&self.directory.join("path_inclusions.json.zst"), facts)
    }

    /// The workspace call-graph facts evaluated under `policy`, if this entry
    /// has them. The graph is a function of the crate IRs and path inclusions,
    /// which the entry's key already covers, and of the policy, which `policy`
    /// names.
    #[cfg(feature = "tracing")]
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure]
    pub(crate) fn read_call_graph(
        &self,
        policy: &str,
    ) -> CordialResult<Option<crate::etiquettes::tracing::CallGraphFacts>> {
        read_json(&self.call_graph_path(policy))
    }

    /// Record the workspace call-graph facts evaluated under `policy`.
    #[cfg(feature = "tracing")]
    #[instrument(level = "debug", skip(self, facts), err(level = "warn"))]
    #[hotpath::measure]
    pub(crate) fn write_call_graph(
        &self,
        policy: &str,
        facts: &crate::etiquettes::tracing::CallGraphFacts,
    ) -> CordialResult<()> {
        self.begin_write()?;
        write_json(&self.call_graph_path(policy), facts)
    }

    #[cfg(feature = "tracing")]
    #[instrument(level = "trace", skip(self))]
    fn call_graph_path(&self, policy: &str) -> PathBuf {
        self.directory.join(format!("call_graph-{policy}.json.zst"))
    }

    /// Inputs that differ from the most recent other entry, for a miss message.
    /// Empty when there is no earlier entry to compare with.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure]
    pub(crate) fn changed_since_last(&self) -> Vec<String> {
        let newest = std::fs::read_dir(&self.root)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|entry| entry.path() != self.directory)
            .filter_map(|entry| {
                let modified = entry.metadata().ok()?.modified().ok()?;
                Some((modified, entry.path()))
            })
            .max();
        let Some((_, previous)) = newest else {
            return Vec::new();
        };
        match read_json::<Inputs>(&previous.join(INPUTS_FILE)) {
            Ok(Some(earlier)) => self.inputs.changed_since(&earlier),
            _ => Vec::new(),
        }
    }

    /// Create this entry's directory, record its inputs, and drop other entries.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure]
    fn begin_write(&self) -> CordialResult<()> {
        if self.pruned.swap(true, Ordering::Relaxed) {
            return Ok(());
        }
        std::fs::create_dir_all(self.directory.join("ir"))?;
        write_json(&self.directory.join(INPUTS_FILE), &self.inputs)?;
        for entry in std::fs::read_dir(&self.root)?.filter_map(Result::ok) {
            if entry.path() != self.directory && entry.path().is_dir() {
                std::fs::remove_dir_all(entry.path())?;
            }
        }
        Ok(())
    }

    #[instrument(level = "trace", skip(self))]
    fn crate_path(&self, crate_name: &str) -> PathBuf {
        self.directory
            .join("ir")
            .join(format!("{crate_name}.ir.zst"))
    }
}

#[instrument(level = "debug", skip(path))]
#[hotpath::measure]
fn partial_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    name.push(".partial");
    path.with_file_name(name)
}

#[instrument(level = "debug", skip(value, path), err(level = "warn"))]
#[hotpath::measure]
fn write_json<T: Serialize>(path: &Path, value: &T) -> CordialResult<()> {
    let partial = partial_path(path);
    let mut encoder =
        zstd::stream::write::Encoder::new(std::fs::File::create(&partial)?, ZSTD_LEVEL)?;
    serde_json::to_writer(&mut encoder, value)?;
    encoder.finish()?;
    std::fs::rename(&partial, path)?;
    Ok(())
}

#[instrument(level = "debug", err(level = "warn"), skip(path))]
#[hotpath::measure]
fn read_json<T: DeserializeOwned>(path: &Path) -> CordialResult<Option<T>> {
    if !path.is_file() {
        return Ok(None);
    }
    let decoder = zstd::stream::read::Decoder::new(std::fs::File::open(path)?)?;
    Ok(Some(serde_json::from_reader(std::io::BufReader::new(
        decoder,
    ))?))
}

/// Lock a mutex, taking the data even if a panicking holder poisoned it: the
/// list of writers is still valid.
#[instrument(level = "trace", skip(mutex))]
fn locked<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
