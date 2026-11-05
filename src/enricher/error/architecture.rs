//! Materialize error architecture policy using the required shared IR evidence.

use crate::CordialResult;
use crate::hooks::{EnrichView, IrEnricher};
use tracing::instrument;

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct ErrorArchitectureInventoryEnricher;

pub(super) static ERROR_ARCHITECTURE_INVENTORY: ErrorArchitectureInventoryEnricher =
    ErrorArchitectureInventoryEnricher;

impl ErrorArchitectureInventoryEnricher {
    pub const ID: &'static str = "error-architecture-inventory";
}

impl IrEnricher for ErrorArchitectureInventoryEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        50
    }

    #[instrument(level = "trace", skip(self, view), err(level = "warn"))]
    #[hotpath::measure(impl_type = "ErrorArchitectureInventoryEnricher")]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let (ir, _, session) = view.into_parts();
        let config = crate::config::load_session_config(session);
        let policy = config.internal_error_chain();
        let inventory = ir.source_error_architecture()?;
        let findings = inventory.assess(
            policy.kind_fanout_max_call_sites(),
            policy.field_strategy(),
            true,
        )?;
        super::compliance::materialize(ir, inventory.crate_root(), &findings)
    }
}
