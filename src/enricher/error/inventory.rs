//! Compatibility hook for remaining error-chain evidence.

use crate::error::CordialResult;
use crate::hooks::{SourceEnrichView, SourceEnricher};
use tracing::instrument;

#[cfg(feature = "error_chain")]
mod chain;

/// Collect remaining error facts and join chain annotations to located site nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct ErrorIrScanEnricher;

impl ErrorIrScanEnricher {
    /// Stable identifier for `ErrorIrScanEnricher`.
    pub const ID: &'static str = "error-ir-scan";
}

impl SourceEnricher for ErrorIrScanEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        50
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich_source(&self, view: SourceEnrichView<'_>) -> CordialResult<()> {
        #[cfg(feature = "error_chain")]
        chain::enrich(view)?;
        #[cfg(not(feature = "error_chain"))]
        let _ = view;
        Ok(())
    }
}
