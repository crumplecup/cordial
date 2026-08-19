//! Anchoring context shared by the chain and compliance layers.

use std::path::PathBuf;

/// Anchoring context shared by the chain and compliance layers: where a
/// finding sits (module/fn path), and which file/crate it belongs to.
/// Compiled only with those layers (`error_chain` / `internal_error_chain`).
pub struct SiteCtx {
    pub context: String,
    pub rel_file: PathBuf,
    pub file: PathBuf,
    pub crate_name: String,
}
