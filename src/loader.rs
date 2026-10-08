use tracing::instrument;
mod populate;
mod scan_roots;
mod source;

use std::path::{Path, PathBuf};

use crate::ir::CrateKind;

#[cfg(any(feature = "_quality_support", feature = "verus_ir"))]
pub use scan_roots::path_has_fixtures;
pub use scan_roots::quality_scan_trees;
pub use source::{SourceFile, SourceLoadView, SourceLoader};

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
