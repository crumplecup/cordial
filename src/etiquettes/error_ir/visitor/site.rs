//! Anchoring context shared by the chain and compliance layers.

use std::path::PathBuf;

/// Anchoring context shared by the chain and compliance layers: where a
/// finding sits (module/fn path), and which file/crate it belongs to.
/// Compiled only with those layers (`error_chain` / `internal_error_chain`).
#[derive(Debug, Clone, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct SiteCtx {
    context: String,
    rel_file: PathBuf,
    file: PathBuf,
    crate_name: String,
}

impl SiteCtx {
    /// Start a builder for this value.
    pub fn builder() -> SiteCtxBuilder {
        SiteCtxBuilder::default()
    }
}
