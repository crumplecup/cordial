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
use std::sync::{Arc, Weak};

use tracing::instrument;

use crate::ir::CrateKind;

use super::{SharedParsedFile, SourceFile, SourceLoadView};

mod session;

pub(in crate::loader::source) use session::CacheToken;
pub(crate) use session::ParsedSourceSession;
use session::SESSION_TOKEN;

type ParsedFiles = HashMap<PathBuf, SharedParsedFile>;
struct ParsedRun {
    token: Weak<CacheToken>,
    files: ParsedFiles,
}

type ParsedRuns = HashMap<u64, ParsedRun>;

thread_local! {
    static PARSED: RefCell<ParsedRuns> = RefCell::new(HashMap::new());
}

impl SourceLoadView {
    /// Project one target from the database of the run this view belongs to.
    /// The view must have been created inside a loaded session.
    #[tracing::instrument(level = "debug", skip(self, target), err(level = "warn"))]
    pub(crate) fn project_target(
        &self,
        target: &crate::loader::CrateTarget,
    ) -> crate::CordialResult<crate::ir::CrateIr> {
        self.cache_token
            .with_database(|database| database.project_target(target))
    }

    /// Project workspace splices and proof calls from the run's database.
    /// The view must have been created inside a loaded session.
    #[cfg(feature = "derives")]
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    pub(crate) fn project_path_inclusions(
        &self,
    ) -> crate::CordialResult<crate::ir::PathInclusionFacts> {
        self.cache_token
            .with_database(|database| database.project_path_inclusions())
    }

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
