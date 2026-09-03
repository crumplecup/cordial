use tracing::instrument;
mod populate;
mod scan_roots;
mod source;

use std::path::{Path, PathBuf};

pub use scan_roots::{path_has_fixtures, quality_scan_trees};
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
#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct CrateTarget {
    /// Cargo package name.
    #[new(into)]
    crate_name: String,
    /// Filesystem path of the crate root.
    #[new(into)]
    crate_root: PathBuf,
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
