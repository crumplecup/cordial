use std::path::PathBuf;
use std::sync::Arc;

use crate::error::CordialResult;
use crate::hooks::{LoadContext, Loader};
use crate::ir::CrateKind;

use super::LoadView;

use tracing::instrument;

mod cache;
mod syntax;
use cache::CacheToken;
pub(crate) use cache::ParsedSourceSession;
pub use syntax::SourceSyntax;
#[cfg(any(
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "crate_attrs"
))]
mod edit;
#[cfg(feature = "internal_error_chain")]
pub(crate) use edit::{SourceEditView, SourceSnapshot};

/// Loads Rust source files into a [`SourceLoadView`].
#[derive(Debug, Default, Clone, Copy)]
pub struct SourceLoader;

impl SourceLoader {
    /// Stable identifier for `SourceLoader`.
    pub const ID: &'static str = "source";
    /// IR attribute key (`ir_origin`).
    pub const ATTR_IR_ORIGIN: &'static str = crate::ir::ATTR_IR_ORIGIN;
    /// Loader origin tag written onto IR nodes.
    pub const ORIGIN: &'static str = crate::ir::ORIGIN_SOURCE;
}

impl Loader for SourceLoader {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn load(&self, view: LoadContext<'_>) -> CordialResult<Box<dyn LoadView>> {
        Ok(Box::new(
            SourceLoadView::for_target(view.target())?
                .with_workspace_root(view.session().project_root().to_path_buf()),
        ))
    }
}

/// Parsed source file retained for IR construction.
#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct SourceFile {
    path: PathBuf,
    source: String,
}

/// Source loader output.
#[derive(Debug, Clone, derive_getters::Getters, derive_setters::Setters)]
#[setters(generate = false, prefix = "with_")]
pub struct SourceLoadView {
    crate_name: String,
    src_root: PathBuf,
    files: Vec<SourceFile>,
    /// Workspace root owning shared manifest policy declarations.
    #[setters(generate)]
    workspace_root: PathBuf,
    crate_kinds: Vec<CrateKind>,
    /// Binary entry paths supplied by the target's Cargo metadata.
    #[setters(generate)]
    binary_entries: Vec<PathBuf>,
    /// Manifest bytes observed alongside Cargo target discovery.
    #[setters(generate)]
    manifest_snapshot: Option<crate::ir::FileSnapshot>,
    #[getter(skip)]
    cache_token: Arc<CacheToken>,
}

impl LoadView for SourceLoadView {
    #[instrument(level = "trace", skip(self))]
    fn loader_id(&self) -> &str {
        SourceLoader::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn crate_name(&self) -> &str {
        &self.crate_name
    }

    #[instrument(level = "trace", skip(self))]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
