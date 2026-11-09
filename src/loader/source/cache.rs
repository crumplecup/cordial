//! Shared parsed-file cache and run-scoped snapshots.

mod compatibility;
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
    feature = "derives",
    feature = "visibility",
    feature = "cli_layout"
))]
mod native;
#[cfg(any(
    feature = "cfg_hygiene",
    feature = "tracing",
    feature = "internal_error_chain",
    feature = "crate_attrs",
    feature = "impl_coverage",
    feature = "amenable_std"
))]
mod text;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use tracing::instrument;

use crate::ir::CrateKind;

use super::{SharedParsedFile, SourceFile, SourceLoadView};

/// Identity of one load-and-probe run, shared by every view created inside it.
/// It owns the run's single source database, created on first use because most
/// views never need one.
pub(super) struct CacheToken(u64, Mutex<Option<crate::loader::SourceDatabase>>);

impl std::fmt::Debug for CacheToken {
    #[tracing::instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("CacheToken").field(&self.0).finish()
    }
}

impl Default for CacheToken {
    #[tracing::instrument(level = "debug")]
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed), Mutex::new(None))
    }
}

impl CacheToken {
    /// Run `f` against this run's database, creating a context-less one if the
    /// run has none yet. The lock is not reentrant: `f` must not request the
    /// database again.
    #[tracing::instrument(level = "trace", skip(self, f))]
    pub(super) fn with_database<R>(
        &self,
        f: impl FnOnce(&mut crate::loader::SourceDatabase) -> R,
    ) -> R {
        let mut slot = self.1.lock().unwrap_or_else(PoisonError::into_inner);
        f(slot.get_or_insert_with(crate::loader::SourceDatabase::new))
    }
}

type ParsedFiles = HashMap<PathBuf, SharedParsedFile>;
struct ParsedRun {
    token: Weak<CacheToken>,
    files: ParsedFiles,
}

type ParsedRuns = HashMap<u64, ParsedRun>;

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
        feature = "cli_layout",
        feature = "derives",
        feature = "tracing",
        feature = "visibility",
        feature = "internal_error_chain",
        feature = "error_sites",
        feature = "glob_imports",
        feature = "inline_tests",
        feature = "pageantry",
        feature = "cfg_hygiene",
        feature = "verus_ir",
        feature = "impl_coverage",
        feature = "amenable_std",
        feature = "allows",
        feature = "panics"
    ))]
    #[instrument(level = "debug")]
    pub(crate) fn for_standalone_scan() -> Self {
        Self::new(String::new(), PathBuf::new(), Vec::new(), Vec::new())
    }

    #[instrument(level = "debug", skip(files, crate_kinds), fields(crate_name = crate_name))]
    pub(in crate::loader) fn new(
        crate_name: String,
        src_root: PathBuf,
        files: Vec<SourceFile>,
        crate_kinds: Vec<CrateKind>,
    ) -> Self {
        Self {
            crate_name,
            workspace_root: src_root.parent().unwrap_or(&src_root).to_path_buf(),
            src_root,
            files,
            crate_kinds,
            binary_entries: Vec::new(),
            manifest_snapshot: None,
            cache_token: SESSION_TOKEN.with(|current| {
                current
                    .borrow()
                    .clone()
                    .unwrap_or_else(|| Arc::new(CacheToken::default()))
            }),
        }
    }
}
