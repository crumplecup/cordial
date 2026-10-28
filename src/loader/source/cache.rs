//! Shared parsed-file cache and run-scoped snapshots.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use sha2::{Digest, Sha256};
use tracing::instrument;

use crate::error::CordialResult;
use crate::ir::CrateKind;
use crate::loader::source_work;

use super::{ParsedSource, SharedParsedFile, SourceFile, SourceLoadView};

#[derive(Debug)]
pub(super) struct CacheToken(u64);

impl Default for CacheToken {
    #[tracing::instrument(level = "debug")]
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

type ParsedFiles = HashMap<PathBuf, SharedParsedFile>;
type ParsedRuns = HashMap<u64, (Weak<CacheToken>, ParsedFiles)>;

thread_local! {
    static PARSED: RefCell<ParsedRuns> = RefCell::new(HashMap::new());
    static SESSION_TOKEN: RefCell<Option<Arc<CacheToken>>> = const { RefCell::new(None) };
}

/// Retain parsed files across all crate views in one load-and-probe run.
pub(crate) struct ParsedSourceSession {
    previous: Option<Arc<CacheToken>>,
    token: Arc<CacheToken>,
}

impl ParsedSourceSession {
    #[tracing::instrument(level = "debug")]
    pub(crate) fn begin() -> Self {
        let token = Arc::new(CacheToken::default());
        let previous = SESSION_TOKEN.with(|current| current.replace(Some(token.clone())));
        Self { previous, token }
    }
}

impl Drop for ParsedSourceSession {
    #[tracing::instrument(level = "trace", skip(self))]
    fn drop(&mut self) {
        #[cfg(feature = "verus_ir")]
        crate::verus_ir::release_shared_ir(self.token.0);
        PARSED.with(|parsed| {
            parsed.borrow_mut().remove(&self.token.0);
        });
        SESSION_TOKEN.with(|current| {
            current.replace(self.previous.take());
        });
    }
}

impl SourceLoadView {
    /// A lazy shared parse cache for a standalone operation outside a session.
    /// Its caller must drop the view before rescanning files it has edited.
    #[cfg(any(
        feature = "antipatterns",
        feature = "derives",
        feature = "tracing",
        feature = "visibility",
        feature = "internal_error_chain",
        feature = "error_sites"
    ))]
    #[instrument(level = "debug")]
    pub(crate) fn for_standalone_scan() -> Self {
        Self::new(String::new(), PathBuf::new(), Vec::new(), Vec::new())
    }

    #[instrument(level = "debug", skip(files, crate_kinds), fields(crate_name = crate_name))]
    pub(super) fn new(
        crate_name: String,
        src_root: PathBuf,
        files: Vec<SourceFile>,
        crate_kinds: Vec<CrateKind>,
    ) -> Self {
        Self {
            crate_name,
            src_root,
            files,
            crate_kinds,
            cache_token: SESSION_TOKEN.with(|current| {
                current
                    .borrow()
                    .clone()
                    .unwrap_or_else(|| Arc::new(CacheToken::default()))
            }),
        }
    }

    /// Return the same parsed file to every hook scanning this path.
    /// Callers own tree discovery and fixture filtering.
    #[tracing::instrument(level = "debug", skip(self, path))]
    #[hotpath::measure(impl_type = "SourceLoadView")]
    pub fn parsed_file(&self, path: &Path, hook: &str) -> CordialResult<SharedParsedFile> {
        PARSED.with(|parsed| {
            let mut parsed = parsed.borrow_mut();
            parsed.retain(|_, (token, _)| token.strong_count() != 0);
            let files = &mut parsed
                .entry(self.cache_token.0)
                .or_insert_with(|| (Arc::downgrade(&self.cache_token), HashMap::new()))
                .1;
            if let Some(file) = files.get(path) {
                debug_assert_eq!(file.inner.session, self.cache_token.0);
                source_work::reuse(path, hook);
                return Ok(file.clone());
            }
            let source = if let Some(file) = self.files.iter().find(|file| file.path() == path) {
                file.source().clone()
            } else {
                source_work::read(path, hook)?
            };
            let syntax = source_work::parse(&source, path, hook)?;
            let digest = Sha256::digest(source.as_bytes()).into();
            let file = SharedParsedFile {
                inner: Rc::new(ParsedSource {
                    path: path.to_path_buf(),
                    source,
                    syntax,
                    digest,
                    session: self.cache_token.0,
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

    /// Identity of the run that issued this parsed-file view.
    #[cfg(any(all(
        feature = "verus_ir",
        any(
            feature = "panics",
            feature = "proof_patterns",
            feature = "verus_warnings"
        )
    )))]
    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn session_id(&self) -> u64 {
        self.cache_token.0
    }
}
