use tracing::instrument;
mod populate;
mod scan_roots;
mod source;

use std::path::{Path, PathBuf};

pub use scan_roots::{path_has_fixtures, quality_scan_trees};
pub use source::{SourceFile, SourceLoadView, SourceLoader};

/// Opaque bundle produced by a loader.
pub trait LoadView: Send + Sync {
    fn loader_id(&self) -> &str;
    fn crate_name(&self) -> &str;
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Target workspace member to analyze.
#[derive(Debug, Clone)]
pub struct CrateTarget {
    pub crate_name: String,
    pub crate_root: PathBuf,
}

impl CrateTarget {
    #[instrument(level = "debug", skip(crate_name, crate_root), ret)]
    pub fn new(crate_name: impl Into<String>, crate_root: impl Into<PathBuf>) -> Self {
        Self {
            crate_name: crate_name.into(),
            crate_root: crate_root.into(),
        }
    }
}

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
