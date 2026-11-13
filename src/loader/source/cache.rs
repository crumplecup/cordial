//! Shared parsed-file cache and run-scoped snapshots.

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

use std::path::PathBuf;
use std::sync::Arc;

use tracing::instrument;

use crate::ir::CrateKind;

use super::{SourceFile, SourceLoadView};

mod session;

pub(in crate::loader::source) use session::CacheToken;
pub(crate) use session::ParsedSourceSession;
use session::SESSION_TOKEN;

impl SourceLoadView {
    /// Project one target from the database of the run this view belongs to.
    /// The view must have been created inside a loaded session.
    #[cfg(feature = "rustdoc")]
    #[tracing::instrument(level = "debug", skip(self, target), err(level = "warn"))]
    #[hotpath::measure]
    pub(crate) fn project_target(
        &self,
        target: &crate::loader::CrateTarget,
    ) -> crate::CordialResult<crate::ir::CrateIr> {
        self.cache_token.project_target(target)
    }

    /// Project workspace splices and proof calls from the run's database.
    /// The view must have been created inside a loaded session.
    #[cfg(feature = "derives")]
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure]
    pub(crate) fn project_path_inclusions(
        &self,
    ) -> crate::CordialResult<crate::ir::PathInclusionFacts> {
        self.cache_token.path_inclusions()
    }

    /// The call-graph facts under `policy`, from the run's cached results when
    /// they hold them, else from `compute`, which is then recorded.
    #[cfg(feature = "tracing")]
    #[tracing::instrument(level = "debug", skip(self, compute), err(level = "warn"))]
    #[hotpath::measure]
    pub(crate) fn cached_call_graph(
        &self,
        policy: &str,
        compute: impl FnOnce() -> crate::CordialResult<crate::etiquettes::tracing::CallGraphFacts>,
    ) -> crate::CordialResult<crate::etiquettes::tracing::CallGraphFacts> {
        self.cache_token.call_graph(policy, compute)
    }

    /// A view that supplies no files and reads through the database of the run
    /// current on this thread. Its caller must drop the view before rereading
    /// files it has edited.
    #[cfg(any(
        feature = "tracing",
        feature = "impl_coverage",
        feature = "amenable_std",
        feature = "homecoming_std"
    ))]
    #[instrument(level = "debug")]
    #[hotpath::measure]
    pub(crate) fn for_session() -> Self {
        Self::new(String::new(), PathBuf::new(), Vec::new(), Vec::new())
    }

    #[instrument(level = "debug", skip(files, crate_kinds), fields(crate_name = crate_name))]
    #[hotpath::measure]
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
