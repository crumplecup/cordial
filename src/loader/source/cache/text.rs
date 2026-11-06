//! Frozen source bytes for IR snapshots and non-syntax inputs.

use super::{PARSED, ParsedRun, SourceLoadView};
use crate::CordialResult;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tracing::instrument;

impl SourceLoadView {
    /// Freeze non-syntax inputs in the same database as the shared tree.
    #[cfg(any(feature = "cfg_hygiene", feature = "tracing"))]
    #[instrument(level = "debug", skip(self, path), err(level = "warn"))]
    pub(crate) fn input_text(&self, path: &Path) -> CordialResult<Option<String>> {
        PARSED.with(|parsed| {
            let mut parsed = parsed.borrow_mut();
            let run = parsed
                .entry(self.cache_token.0)
                .or_insert_with(|| ParsedRun {
                    token: Arc::downgrade(&self.cache_token),
                    database: crate::loader::SourceDatabase::new(),
                    files: HashMap::new(),
                });
            run.database.input_text(path)
        })
    }

    /// Read frozen database text for an IR snapshot without requesting syntax.
    #[cfg(any(
        feature = "internal_error_chain",
        feature = "tracing",
        feature = "crate_attrs",
        feature = "impl_coverage",
        feature = "amenable_std"
    ))]
    #[instrument(level = "debug", skip(self, path), err(level = "warn"))]
    pub(crate) fn source_text(&self, path: &Path) -> CordialResult<String> {
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
            run.database
                .captured_text(path, supplied)?
                .ok_or_else(|| crate::CordialError::not_found(path.to_path_buf()))
        })
    }
}
