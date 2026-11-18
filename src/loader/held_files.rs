//! The Rust files the database holds under a crate's root, as the collectors that scan the crate
//! are told them: sorted, and answered without the file system.

use std::path::{Path, PathBuf};

/// The `.rs` files under a crate's root that the database holds, sorted by path (the order a walk
/// that sorts each directory by name visits them).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct HeldFiles {
    paths: Vec<PathBuf>,
}

impl HeldFiles {
    #[tracing::instrument(level = "trace", skip(paths))]
    pub(crate) fn new(mut paths: Vec<PathBuf>) -> Self {
        paths.sort();
        paths.dedup();
        Self { paths }
    }

    /// The files under `dir`, in order.
    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn under<'a>(&'a self, dir: &'a Path) -> impl Iterator<Item = &'a PathBuf> + 'a {
        self.paths.iter().filter(move |path| path.starts_with(dir))
    }

    /// Whether the database holds a file under `dir`: the directory exists, as far as the
    /// collectors can tell.
    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn has_dir(&self, dir: &Path) -> bool {
        self.under(dir).next().is_some()
    }

    /// Whether the database holds the file at `path`.
    #[cfg(feature = "tracing")]
    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn has(&self, path: &Path) -> bool {
        self.paths
            .binary_search_by(|held| held.as_path().cmp(path))
            .is_ok()
    }

    /// The trees the quality collectors scan: `src` and `tests`, of those that hold a file.
    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn scan_trees(&self, root: &Path) -> Vec<PathBuf> {
        ["src", "tests"]
            .into_iter()
            .map(|sub| root.join(sub))
            .filter(|tree| self.has_dir(tree))
            .collect()
    }
}
