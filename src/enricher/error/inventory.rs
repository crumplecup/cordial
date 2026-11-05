//! Compatibility hook for remaining internal architecture and apply evidence.

use crate::error::CordialResult;
use crate::hooks::{SourceEnrichView, SourceEnricher};
use tracing::instrument;

#[cfg(feature = "internal_error_chain")]
mod internal;

/// Collect internal architecture, compliance and apply evidence pending migration.
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
        #[cfg(feature = "internal_error_chain")]
        internal::enrich(view)?;
        #[cfg(not(feature = "internal_error_chain"))]
        let _ = view;
        Ok(())
    }
}
