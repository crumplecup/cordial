//! Compatibility syntax access while the remaining collectors migrate.

use super::{PARSED, ParsedRun, SourceLoadView};
use crate::error::CordialResult;
use crate::loader::source::{ParsedSource, SharedParsedFile};
use crate::loader::source_work;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

impl SourceLoadView {
    /// Issue a shared tree for an explicitly supplied standalone source fixture.
    #[cfg(any(feature = "error_chain", feature = "panics"))]
    #[tracing::instrument(level = "debug", skip(source, path), err(level = "warn"))]
    pub(crate) fn parse_fixture(path: &Path, source: &str) -> CordialResult<SharedParsedFile> {
        use crate::loader::source::SourceFile;
        use std::path::PathBuf;

        let view = Self::new(
            String::new(),
            PathBuf::new(),
            vec![SourceFile::new(path.to_path_buf(), source.to_owned())],
            Vec::new(),
        );
        view.parsed_file(path, "source-fixture")
    }

    /// Return the same parsed file to every hook scanning this path.
    /// Callers own tree discovery and fixture filtering.
    #[tracing::instrument(level = "debug", skip(self, path))]
    #[hotpath::measure(impl_type = "SourceLoadView")]
    pub fn parsed_file(&self, path: &Path, hook: &str) -> CordialResult<SharedParsedFile> {
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
            let files = &mut run.files;
            if let Some(file) = files.get(path) {
                debug_assert_eq!(file.inner.session, self.cache_token.0);
                source_work::reuse(path, hook);
                return Ok(file.clone());
            }
            let supplied = self
                .files
                .iter()
                .find(|file| file.path() == path)
                .map(|file| file.source().as_str());
            let source = run
                .database
                .captured_text(path, supplied)?
                .ok_or_else(|| crate::CordialError::not_found(path.to_path_buf()))?;
            // Retire this compatibility AST as collectors move to database
            // syntax. Semantic collection must not parse these inputs twice.
            let syntax = source_work::parse(&source, path, hook)?;
            let digest = Sha256::digest(source.as_bytes()).into();
            let file = SharedParsedFile {
                inner: Rc::new(ParsedSource {
                    path: path.to_path_buf(),
                    source,
                    syntax,
                    digest,
                    session: self.cache_token.0,
                    macros: std::cell::OnceCell::new(),
                }),
            };
            files.insert(path.to_path_buf(), file.clone());
            Ok(file)
        })
    }

    /// Walk Rust files under a root and issue their shared parsed snapshots.
    /// The caller decides which paths belong to its scan.
    #[tracing::instrument(level = "debug", skip(self, include), err(level = "warn"))]
    pub fn parsed_rust_files_under(
        &self,
        root: &Path,
        hook: &str,
        mut include: impl FnMut(&Path) -> bool,
    ) -> CordialResult<Vec<SharedParsedFile>> {
        let mut files = Vec::new();
        if !root.is_dir() {
            return Ok(files);
        }
        for entry in walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let path = entry.path();
            if path.extension().is_some_and(|extension| extension == "rs") && include(path) {
                files.push(self.parsed_file(path, hook)?);
            }
        }
        Ok(files)
    }
}
