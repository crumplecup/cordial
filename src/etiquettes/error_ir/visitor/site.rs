//! Anchoring context shared by the chain and compliance layers.

use std::path::PathBuf;

/// Anchoring context shared by the chain and compliance layers: where a
/// finding sits (module/fn path), and which file/crate it belongs to.
/// Plain data, no feature-gated types, so it lives here unconditionally
/// and both layer modules can depend on it freely. Unused (and allowed to
/// be so) when neither layer is compiled in.
#[allow(dead_code)]
pub struct SiteCtx {
    pub context: String,
    pub rel_file: PathBuf,
    pub file: PathBuf,
    pub crate_name: String,
}
