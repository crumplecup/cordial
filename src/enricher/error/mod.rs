//! Shared error IR enricher stack for the error-handling family.
//!
//! All error-handling etiquettes register [`ERROR_IR_ENRICHERS`] so the session
//! builds one graph foundation (unified scan → partition/foreign → attenuation)
//! instead of each etiquette declaring overlapping enrichers.

use tracing::instrument;
mod inventory;
mod scan;

pub use inventory::ErrorIrScanEnricher;
pub use scan::{ErrorIrScanReport, scan_crate_error_ir};

use crate::enricher::{AttributeEnricher, ErrorFlowEnricher, ScopeEnricher};
use crate::hooks::IrEnricher;
use crate::plugin::ErrorHandlingLayers;

static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static ERROR_IR_SCAN: ErrorIrScanEnricher = ErrorIrScanEnricher;
static ERROR_FLOW: ErrorFlowEnricher = ErrorFlowEnricher;

/// Foreign-error-attenuation inventory enricher, gated as a whole unit —
/// see `docs/planning/cfg-scatter-etiquette.md` for the pattern.
#[cfg(feature = "foreign_error_attenuation")]
mod attenuation {
    pub(super) use crate::etiquettes::foreign_error_attenuation::ForeignErrorAttenuationInventoryEnricher;

    pub(super) static FOREIGN_ERROR_ATTENUATION_INVENTORY:
        ForeignErrorAttenuationInventoryEnricher = ForeignErrorAttenuationInventoryEnricher;
}

/// Canonical enricher stack for error-handling runs (feature-gated layers).
///
/// Order after session priority sort: scope → attribute (6) → error-ir-scan (50) →
/// error-flow (51) → attenuation (52).
pub static ERROR_IR_ENRICHERS: &[&'static dyn IrEnricher] = &[
    &SCOPE_ENRICHER,
    #[cfg(feature = "error_sites")]
    &ERROR_IR_SCAN,
    #[cfg(feature = "error_sites")]
    &ERROR_FLOW,
    #[cfg(feature = "foreign_error_attenuation")]
    &attenuation::FOREIGN_ERROR_ATTENUATION_INVENTORY,
    &ATTRIBUTE_ENRICHER,
];

/// Enricher ids in the shared stack for the given policy layers and enabled features.
#[instrument(level = "debug")]
pub fn error_ir_enricher_ids(layers: ErrorHandlingLayers) -> Vec<&'static str> {
    let mut ids = vec![ScopeEnricher::ID];
    if layers.sites || layers.chain || layers.internal {
        #[cfg(feature = "error_sites")]
        {
            ids.push(ErrorIrScanEnricher::ID);
            ids.push(ErrorFlowEnricher::ID);
        }
    }
    if layers.attenuation {
        #[cfg(feature = "foreign_error_attenuation")]
        ids.push(attenuation::ForeignErrorAttenuationInventoryEnricher::ID);
    }
    ids.push(AttributeEnricher::ID);
    ids
}
