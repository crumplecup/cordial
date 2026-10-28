//! Crate-tree scan for `pub mod` paths that do not earn their visibility.

use std::path::{Path, PathBuf};

use crate::error::CordialResult;
use crate::loader::SourceLoadView;

use super::types::{VisibilityRecord, VisibilityThresholds};

use tracing::instrument;
mod eval;
mod findings;
mod tree;
mod vis;

use eval::resolve_eval;
use findings::collect_findings;
use tree::scan_shared_module_file;
use vis::VisKind;

/// Walk the crate's root module tree (`src/lib.rs` or `src/main.rs`) and
/// apply `thresholds`. The scanner never picks thresholds itself.
#[instrument(level = "debug", skip(thresholds), err(level = "warn"))]
pub fn scan_crate_visibility(
    crate_root: &Path,
    thresholds: VisibilityThresholds,
) -> CordialResult<Vec<VisibilityRecord>> {
    let source = SourceLoadView::for_standalone_scan();
    scan_shared_crate_visibility(&source, crate_root, thresholds)
}

#[instrument(level = "debug", skip(source, thresholds), err(level = "warn"))]
pub(super) fn scan_shared_crate_visibility(
    source: &SourceLoadView,
    crate_root: &Path,
    thresholds: VisibilityThresholds,
) -> CordialResult<Vec<VisibilityRecord>> {
    let Some(root_file) = crate_root_file(crate_root) else {
        return Ok(Vec::new());
    };
    let root = scan_shared_module_file(
        source,
        &root_file,
        "crate".to_string(),
        VisKind::Pub,
        true,
        true,
        true,
    )?;
    let eval = resolve_eval(&root, thresholds.clone());
    collect_findings(&root, thresholds, eval)
}

#[instrument(level = "debug")]
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
