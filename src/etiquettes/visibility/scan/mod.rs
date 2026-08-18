//! Crate-tree scan for `pub mod` paths that do not earn their visibility.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::CordialResult;

use super::types::{VisibilityRecord, VisibilityThresholds};

use tracing::instrument;
mod eval;
mod findings;
mod tree;
mod vis;

use eval::resolve_eval;
use findings::collect_findings;
use tree::scan_module_file;
use vis::VisKind;

/// Cached branching floor from a previous peel. Digest is a hash of the
/// scanned crate files; a source edit invalidates it and forces a re-peel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchingCache {
    pub digest: String,
    pub floor: usize,
}

impl BranchingCache {
    #[instrument(level = "debug", skip(path))]
    pub fn load(path: &Path) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    #[instrument(level = "debug", skip(self, path), err(level = "warn"))]
    pub fn write(&self, path: &Path) -> CordialResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

/// Walk the crate's root module tree (`src/lib.rs` or `src/main.rs`) and
/// apply `thresholds`. The scanner never picks thresholds itself.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_visibility(
    crate_root: &Path,
    thresholds: VisibilityThresholds,
) -> CordialResult<Vec<VisibilityRecord>> {
    Ok(scan_crate_visibility_with_cache(crate_root, thresholds, None)?.0)
}

/// Same as [`scan_crate_visibility`], but reuses a branching floor when the
/// crate-file digest still matches. On mismatch (or first run) this peels,
/// writes a new cache payload, then applies the lowered floor — a two-pass
/// analysis so undersized peeled modules do not fire `VIS-MOD-THIN-001`.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_visibility_with_cache(
    crate_root: &Path,
    thresholds: VisibilityThresholds,
    cached: Option<BranchingCache>,
) -> CordialResult<(Vec<VisibilityRecord>, Option<BranchingCache>)> {
    let Some(root_file) = crate_root_file(crate_root) else {
        return Ok((Vec::new(), None));
    };
    let root = scan_module_file(
        &root_file,
        "crate".to_string(),
        VisKind::Pub,
        true,
        true,
        true,
    )?;
    let (eval, new_cache) = resolve_eval(&root, thresholds, cached);
    Ok((collect_findings(&root, thresholds, eval), new_cache))
}

fn crate_root_file(crate_root: &Path) -> Option<PathBuf> {
    let src_lib = crate_root.join("src").join("lib.rs");
    if src_lib.is_file() {
        return Some(src_lib);
    }
    let src_main = crate_root.join("src").join("main.rs");
    if src_main.is_file() {
        return Some(src_main);
    }
    let lib = crate_root.join("lib.rs");
    if lib.is_file() {
        return Some(lib);
    }
    None
}
