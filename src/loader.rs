#[cfg(any(feature = "cli_layout", feature = "internal_error_chain"))]
pub(crate) use native_syntax::module_path as database_module_path;
use tracing::instrument;
mod database;
mod native_syntax;
pub use database::{SemanticCargoOptions, SourceDatabase};
#[cfg(any(
    feature = "tracing",
    feature = "internal_error_chain",
    feature = "panics",
    feature = "antipatterns"
))]
pub(crate) use native_syntax::DatabaseMacroSyntax;
pub(crate) use native_syntax::DatabaseSource;
#[cfg(all(feature = "verus_ir", feature = "visibility"))]
pub(crate) use native_syntax::DatabaseVerusSyntax;
#[cfg(any(
    feature = "panics",
    feature = "antipatterns",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "visibility",
    feature = "cli_layout",
))]
pub(crate) use native_syntax::token_text as database_token_text;
#[cfg(feature = "antipatterns")]
pub(crate) use native_syntax::token_text_excluding as database_token_text_excluding;
mod library_root;
pub use library_root::library_root_rs;
mod populate;
mod scan_roots;
mod source;
mod source_work;

use std::path::{Path, PathBuf};

use crate::ir::CrateKind;

#[cfg(any(feature = "_quality_support", feature = "verus_ir"))]
pub use scan_roots::path_has_fixtures;
pub use scan_roots::quality_scan_trees;
pub(crate) use source::ParsedSourceSession;
#[cfg(feature = "internal_error_chain")]
pub(crate) use source::{SourceEditView, SourceSnapshot};
pub use source::{SourceFile, SourceLoadView, SourceLoader, SourceSyntax};
/// Opaque bundle produced by a loader.
pub trait LoadView: Send + Sync {
    /// Loader id.
    fn loader_id(&self) -> &str;
    /// Package name this IR belongs to.
    fn crate_name(&self) -> &str;
    /// As any.
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Target workspace member to analyze.
#[derive(Debug, Clone, derive_new::new, derive_getters::Getters, derive_setters::Setters)]
#[setters(prefix = "with_")]
pub struct CrateTarget {
    /// Cargo package name.
    #[new(into)]
    #[setters(skip)]
    crate_name: String,
    /// Filesystem path of the crate root.
    #[new(into)]
    #[setters(skip)]
    crate_root: PathBuf,
    /// Cargo target kinds; empty when no Cargo metadata was available.
    #[new(default)]
    kinds: Vec<CrateKind>,
    /// Binary entry paths supplied by Cargo metadata, not inferred from names.
    #[new(default)]
    binary_entries: Vec<PathBuf>,
    /// Manifest identity observed alongside the target's Cargo metadata.
    #[new(default)]
    manifest_snapshot: Option<crate::ir::FileSnapshot>,
}

impl CrateTarget {
    /// Whether Cargo builds this package as a proc-macro library.
    #[instrument(level = "trace", skip(self))]
    pub fn is_proc_macro(&self) -> bool {
        self.kinds.contains(&CrateKind::ProcMacro)
    }
}

/// Map a file under `src/` to its module path segments.
#[instrument(level = "debug", skip(file))]
pub fn module_path_from_src_file(src_root: &Path, file: &Path) -> Vec<String> {
    let Ok(rel) = file.strip_prefix(src_root) else {
        return Vec::new();
    };
    let rel = rel.with_extension("");
    if rel.as_os_str().is_empty() || rel == Path::new("lib") || rel == Path::new("main") {
        return Vec::new();
    }
    let mut parts: Vec<String> = rel
        .components()
        .filter_map(|component| component.as_os_str().to_str().map(str::to_string))
        .collect();
    if parts.last().is_some_and(|part| part == "mod") {
        parts.pop();
    }
    parts
}
