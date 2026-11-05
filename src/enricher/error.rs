//! Shared error IR enricher stack for the error-handling family.
//!
//! All error-handling etiquettes register [`ERROR_IR_ENRICHERS`] so the session
//! builds one graph foundation (unified scan → partition/foreign → attenuation)
//! instead of each etiquette declaring overlapping enrichers.

use tracing::instrument;
#[cfg(feature = "error_chain")]
mod chains;
mod inventory;
mod scan;
mod sites;

pub use inventory::ErrorIrScanEnricher;
#[cfg(feature = "internal_error_chain")]
pub(crate) use scan::compliance_sort::scan_crate_error_ir_with_absolute_paths;
pub use scan::{ErrorIrScanReport, scan_crate_error_ir};
use sites::ErrorSiteInventoryEnricher;

use crate::enricher::{AttributeEnricher, ErrorFlowEnricher, ScopeEnricher};
use crate::hooks::IrEnricher;
use crate::plugin::ErrorHandlingLayers;

static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static ERROR_IR_SCAN: ErrorIrScanEnricher = ErrorIrScanEnricher;
static ERROR_SITE_INVENTORY: ErrorSiteInventoryEnricher = ErrorSiteInventoryEnricher;
#[cfg(feature = "error_chain")]
static ERROR_CHAIN_INVENTORY: chains::ErrorChainInventoryEnricher =
    chains::ErrorChainInventoryEnricher;
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
/// Order after session priority sort: scope → attribute (6) → site inventory (49) → chain inventory / error-ir-scan (50) →
/// error-flow (51) → attenuation (52).
pub static ERROR_IR_ENRICHERS: &[&'static dyn IrEnricher] = &[
    &SCOPE_ENRICHER,
    #[cfg(feature = "error_sites")]
    &ERROR_SITE_INVENTORY,
    #[cfg(feature = "error_chain")]
    &ERROR_CHAIN_INVENTORY,
    #[cfg(feature = "error_sites")]
    &ERROR_IR_SCAN,
    #[cfg(feature = "error_sites")]
    &ERROR_FLOW,
    #[cfg(feature = "foreign_error_attenuation")]
    &attenuation::FOREIGN_ERROR_ATTENUATION_INVENTORY,
    &ATTRIBUTE_ENRICHER,
];

/// Enricher ids in the shared stack for the given policy layers and enabled features.
#[instrument(level = "debug", skip(layers))]
pub fn error_ir_enricher_ids(layers: ErrorHandlingLayers) -> Vec<&'static str> {
    let mut ids = vec![ScopeEnricher::ID];
    if layers.sites() || layers.chain() || layers.internal() {
        #[cfg(feature = "error_sites")]
        {
            ids.push(ErrorSiteInventoryEnricher::ID);
            ids.push(ErrorIrScanEnricher::ID);
            ids.push(ErrorFlowEnricher::ID);
        }
    }
    #[cfg(feature = "error_chain")]
    if layers.sites() || layers.chain() || layers.internal() {
        ids.push(chains::ErrorChainInventoryEnricher::ID);
    }
    if layers.attenuation() {
        #[cfg(feature = "foreign_error_attenuation")]
        ids.push(attenuation::ForeignErrorAttenuationInventoryEnricher::ID);
    }
    ids.push(AttributeEnricher::ID);
    ids
}
