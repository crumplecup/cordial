//! Native syntax collection through the run's canonical source database.

use super::{PARSED, ParsedRun, SourceFile, SourceLoadView};
use crate::{CordialResult, loader::DatabaseSource};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

impl SourceLoadView {
    /// Collect facts while the database owns both the source and its tree.
    /// The callback cannot return syntax borrowed from this input owner.
    #[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
    pub(crate) fn collect_native_file<T>(
        &self,
        path: &Path,
        collect: impl FnOnce(&DatabaseSource<'_>) -> CordialResult<T>,
    ) -> CordialResult<T> {
        PARSED.with(|parsed| {
            let mut parsed = parsed.borrow_mut();
            parsed.retain(|_, run| run.token.strong_count() != 0);
            let run = parsed
                .entry(self.cache_token.0)
                .or_insert_with(|| ParsedRun {
                    token: Arc::downgrade(&self.cache_token),
                    database: crate::loader::SourceDatabase::new(),
                    files: HashMap::new(),
                });
            let supplied = self
                .files
                .iter()
                .find(|file| file.path() == path)
                .map(|file| file.source().as_str());
            run.database.collect_source(path, supplied, collect)
        })
    }

    /// Supplied text uses the ordinary session owner, even when no file exists.
    #[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
    pub(crate) fn collect_native_fixture<T>(
        path: &Path,
        source: &str,
        collect: impl FnOnce(&DatabaseSource<'_>) -> CordialResult<T>,
    ) -> CordialResult<T> {
        Self::new(
            String::new(),
            PathBuf::new(),
            vec![SourceFile::new(path.to_path_buf(), source.to_owned())],
            Vec::new(),
        )
        .collect_native_file(path, collect)
    }
}
