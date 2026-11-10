//! Native syntax collection through the run's canonical source database.

use super::SourceLoadView;
use crate::{CordialResult, loader::DatabaseSource};
use std::path::Path;

impl SourceLoadView {
    /// Collect facts while the database owns both the source and its tree.
    /// The callback cannot return syntax borrowed from this input owner.
    #[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
    pub(crate) fn collect_native_file<T>(
        &self,
        path: &Path,
        collect: impl FnOnce(&DatabaseSource<'_>) -> CordialResult<T>,
    ) -> CordialResult<T> {
        let supplied = self
            .files
            .iter()
            .find(|file| file.path() == path)
            .map(|file| file.source().as_str());
        self.cache_token
            .with_database(|database| database.collect_source(path, supplied, collect))
    }

    /// Supplied text uses the ordinary session owner, even when no file exists.
    #[cfg(any(
        feature = "antipatterns",
        feature = "error_sites",
        feature = "tracing",
        feature = "allows",
        feature = "cfg_hygiene",
        feature = "cfg_scatter",
        feature = "glob_imports",
        feature = "inline_tests",
        feature = "modularity",
        feature = "pageantry",
        feature = "derives"
    ))]
    #[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
    pub(crate) fn collect_native_fixture<T>(
        path: &Path,
        source: &str,
        collect: impl FnOnce(&DatabaseSource<'_>) -> CordialResult<T>,
    ) -> CordialResult<T> {
        use super::SourceFile;
        use std::path::PathBuf;

        Self::new(
            String::new(),
            PathBuf::new(),
            vec![SourceFile::new(path.to_path_buf(), source.to_owned())],
            Vec::new(),
        )
        .collect_native_file(path, collect)
    }
}
