//! Plan and remove only regenerable Cordial store artifacts.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tracing::instrument;

use crate::{CordialResult, ProgressSink, StoreLayout, SysrootCache, project_slug_from_path};

/// Stores to consider for a clean operation.
#[derive(Debug, Clone)]
pub enum CleanScope {
    /// One project's store.
    Project {
        /// Store layout to clean.
        store: StoreLayout,
        /// Source root that must not be contained by the store.
        project_root: PathBuf,
    },
    /// Recognized stores directly under a store home, plus the shared sysroot.
    All {
        /// Parent of the project stores and shared sysroot.
        store_home: PathBuf,
        /// Source root that must not be contained by a store.
        project_root: PathBuf,
    },
}

/// A path selected for removal, with its size measured before deletion.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct CleanPath {
    /// Recognized store containing this path.
    store: PathBuf,
    /// Exact path to remove.
    path: PathBuf,
    /// Bytes occupied by regular files in this path.
    #[getter(copy)]
    bytes: u64,
}

/// A store that was not selected, with the reason.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct SkippedStore {
    /// Store root.
    path: PathBuf,
    /// Explanation for skipping it.
    reason: String,
}

/// Fixed deletion plan, also used by dry run output.
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct CleanPlan {
    /// Recognized stores, including stores with no current regenerable paths.
    stores: Vec<PathBuf>,
    /// Paths to remove.
    paths: Vec<CleanPath>,
    /// Stores skipped during recognition or safety checks.
    skipped: Vec<SkippedStore>,
    /// Number of checked-off items in discarded worklists.
    #[getter(copy)]
    checked_off: usize,
    /// Checked-off counts by store, so excluding a busy store also excludes its count.
    checked_off_by_store: BTreeMap<PathBuf, usize>,
}

/// One deletion failure; other planned paths are still attempted.
#[derive(Debug, derive_getters::Getters)]
pub struct CleanFailure {
    /// Failed path.
    path: PathBuf,
    /// Filesystem error.
    error: io::Error,
}

/// Result of executing a clean plan.
#[derive(Debug, derive_getters::Getters)]
pub struct CleanReport {
    /// Paths successfully removed.
    removed: Vec<CleanPath>,
    /// Paths that could not be removed.
    failures: Vec<CleanFailure>,
    /// Stores skipped by the plan.
    skipped: Vec<SkippedStore>,
    /// Number of checked-off items discarded.
    #[getter(copy)]
    checked_off: usize,
}

impl CleanPlan {
    /// Exclude a store whose lock could not be held.
    #[cfg(feature = "cli")]
    #[instrument(level = "debug", skip(self))]
    pub(crate) fn exclude_store(&mut self, root: &Path, reason: &str) {
        self.paths.retain(|path| path.store != root);
        self.stores.retain(|store| store != root);
        if let Some(count) = self.checked_off_by_store.remove(root) {
            self.checked_off -= count;
        }
        self.skipped.push(SkippedStore {
            path: root.to_path_buf(),
            reason: reason.to_owned(),
        });
    }

    /// Total estimated bytes in the plan.
    #[instrument(level = "trace", skip(self))]
    pub fn total_bytes(&self) -> u64 {
        self.paths.iter().map(|path| path.bytes).sum()
    }
}

impl CleanReport {
    /// Sum of measured sizes for paths successfully removed.
    #[instrument(level = "trace", skip(self))]
    pub fn freed_bytes(&self) -> u64 {
        self.removed.iter().map(|path| path.bytes).sum()
    }

    /// Whether every recognized store and path was cleaned.
    #[instrument(level = "trace", skip(self))]
    pub fn complete(&self) -> bool {
        self.failures.is_empty()
            && self
                .skipped
                .iter()
                .all(|store| store.reason.starts_with("unrecognized"))
    }
}

/// Classify stores and measure the exact allowlisted paths to remove.
#[instrument(level = "debug", skip(scope), err(level = "warn"))]
pub fn plan_clean(scope: CleanScope) -> CordialResult<CleanPlan> {
    let mut plan = CleanPlan {
        stores: Vec::new(),
        paths: Vec::new(),
        skipped: Vec::new(),
        checked_off: 0,
        checked_off_by_store: BTreeMap::new(),
    };
    match scope {
        CleanScope::Project {
            store,
            project_root,
        } => {
            plan_project(&mut plan, &store, &project_root)?;
        }
        CleanScope::All {
            store_home,
            project_root,
        } => {
            if store_home.exists() {
                for entry in fs::read_dir(&store_home)? {
                    let entry = entry?;
                    let path = entry.path();
                    if entry.file_name() == "sysroot" {
                        continue;
                    }
                    if entry.file_type()?.is_file() {
                        plan.skipped.push(SkippedStore {
                            path,
                            reason: "unrecognized store".into(),
                        });
                        continue;
                    }
                    let store = StoreLayout::from_root(&path, project_slug_from_path(&path));
                    plan_project(&mut plan, &store, &project_root)?;
                }
            }
            plan_sysroot(
                &mut plan,
                &SysrootCache::from_home(store_home),
                &project_root,
            )?;
        }
    }
    Ok(plan)
}

#[instrument(level = "debug", skip(plan, store), err(level = "warn"))]
fn plan_project(
    plan: &mut CleanPlan,
    store: &StoreLayout,
    project_root: &Path,
) -> CordialResult<()> {
    let root = store.root();
    if !root.exists() {
        return Ok(());
    }
    if let Some(reason) = refusal(root, project_root)? {
        plan.skipped.push(SkippedStore {
            path: root.clone(),
            reason,
        });
        return Ok(());
    }
    let recognized = regular_file(&store.marker_path())?
        || [
            store.cache_dir(),
            store.findings_dir(),
            store.exceptions_dir(),
        ]
        .iter()
        .all(|path| real_dir(path));
    if !recognized {
        plan.skipped.push(SkippedStore {
            path: root.clone(),
            reason: "unrecognized store".into(),
        });
        return Ok(());
    }
    plan.stores.push(root.clone());
    for path in store.regenerable_paths() {
        #[cfg(feature = "quality")]
        if path == store.worklist_path() && path.exists() {
            if let Ok(Some(list)) = crate::Worklist::read(store) {
                let count = list
                    .items()
                    .iter()
                    .filter(|item| *item.state() == crate::ItemState::Done)
                    .count();
                plan.checked_off += count;
                plan.checked_off_by_store.insert(root.clone(), count);
            }
        }
        add_path(plan, root, path)?;
    }
    Ok(())
}

#[instrument(level = "debug", skip(plan, sysroot), err(level = "warn"))]
fn plan_sysroot(
    plan: &mut CleanPlan,
    sysroot: &SysrootCache,
    project_root: &Path,
) -> CordialResult<()> {
    let root = sysroot.root();
    if !root.exists() {
        return Ok(());
    }
    if let Some(reason) = refusal(root, project_root)? {
        plan.skipped.push(SkippedStore {
            path: root.clone(),
            reason,
        });
        return Ok(());
    }
    let recognized = regular_file(&sysroot.marker_path())?
        || (real_dir(&sysroot.rustdoc_cache_dir()) && real_dir(&sysroot.builds_dir()));
    if !recognized {
        plan.skipped.push(SkippedStore {
            path: root.clone(),
            reason: "unrecognized sysroot".into(),
        });
        return Ok(());
    }
    plan.stores.push(root.clone());
    for path in sysroot.regenerable_paths() {
        add_path(plan, root, path)?;
    }
    Ok(())
}

#[instrument(level = "debug", skip(plan, path), err(level = "warn"))]
fn add_path(plan: &mut CleanPlan, root: &Path, path: PathBuf) -> CordialResult<()> {
    match fs::symlink_metadata(&path) {
        Ok(_) => plan.paths.push(CleanPath {
            store: root.to_path_buf(),
            bytes: size_of(&path)?,
            path,
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

#[instrument(level = "debug", skip(path), err(level = "warn"))]
fn size_of(path: &Path) -> io::Result<u64> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        return Ok(metadata.len());
    }
    let mut bytes = 0;
    for entry in fs::read_dir(path)? {
        bytes += size_of(&entry?.path())?;
    }
    Ok(bytes)
}

#[instrument(level = "trace", err(level = "warn"))]
fn refusal(root: &Path, project_root: &Path) -> CordialResult<Option<String>> {
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_dir() {
        return Ok(Some("store root is not a real directory".into()));
    }
    let root = root.canonicalize()?;
    let project_root = project_root.canonicalize()?;
    if project_root.starts_with(&root) {
        return Ok(Some("store contains the project source root".into()));
    }
    Ok(None)
}

#[instrument(level = "debug", skip(path))]
fn real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_dir())
}

#[instrument(level = "debug", skip(path), err(level = "warn"))]
fn regular_file(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.file_type().is_file()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Execute an allowlisted plan, continuing after individual removal failures.
#[instrument(level = "info", skip(plan))]
pub fn run_clean(plan: CleanPlan) -> CleanReport {
    run_clean_with_progress(plan, crate::noop_progress())
}

/// Execute a plan while reporting the number of paths processed.
#[instrument(level = "info", skip(plan, progress))]
pub fn run_clean_with_progress(plan: CleanPlan, progress: &dyn ProgressSink) -> CleanReport {
    let task = progress.bar("Cleaning store artifacts".into(), plan.paths.len() as u64);
    let mut report = CleanReport {
        removed: Vec::new(),
        failures: Vec::new(),
        skipped: plan.skipped,
        checked_off: plan.checked_off,
    };
    for target in plan.paths {
        task.set_message(format!("Removing {}", target.path.display()));
        let result = match fs::symlink_metadata(&target.store) {
            Ok(metadata) if metadata.file_type().is_dir() => remove_path(&target.path),
            Ok(_) => Err(io::Error::other("store root changed type since planning")),
            Err(error) => Err(error),
        };
        match result {
            Ok(()) => report.removed.push(target),
            Err(error) => report.failures.push(CleanFailure {
                path: target.path,
                error,
            }),
        }
        task.inc(1);
    }
    task.finish(format!(
        "Processed {} path(s)",
        report.removed.len() + report.failures.len()
    ));
    report
}

#[instrument(level = "debug", skip(path), err(level = "warn"))]
fn remove_path(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}
