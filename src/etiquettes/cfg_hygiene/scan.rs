//! Typed cfg occurrences and standalone fixture adapter.
pub use crate::ir::scan_cfg_hygiene_source as scan_rust_source;
use std::path::PathBuf;
use tracing::instrument;

/// One cfg name mentioned by a `#[cfg(...)]`/`#[cfg_attr(...)]` attribute.
/// A single attribute mentioning several names (e.g. `any(kani, creusot)`)
/// produces one occurrence per name.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    derive_builder::Builder,
    derive_getters::Getters,
)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct CfgNameOccurrence {
    name: String,
    context: String,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    snippet: String,
}

impl CfgNameOccurrence {
    #[instrument(level = "debug")]
    /// Construct one located cfg-name occurrence.
    pub fn builder() -> CfgNameOccurrenceBuilder {
        CfgNameOccurrenceBuilder::default()
    }
}
