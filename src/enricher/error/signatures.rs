//! Materialize loader-owned signatures without source access.

use crate::CordialResult;
use crate::hooks::{EnrichView, IrEnricher};
use tracing::instrument;

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct ErrorSignatureInventoryEnricher;

pub(super) static ERROR_SIGNATURE_INVENTORY: ErrorSignatureInventoryEnricher =
    ErrorSignatureInventoryEnricher;

impl ErrorSignatureInventoryEnricher {
    pub const ID: &'static str = "error-signature-inventory";
}

impl IrEnricher for ErrorSignatureInventoryEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        50
    }

    #[instrument(level = "trace", skip(self, view), err(level = "warn"))]
    #[hotpath::measure(impl_type = "ErrorSignatureInventoryEnricher")]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let ir = view.ir();
        let inventory = ir.source_error_signatures()?;
        let assessment = inventory.assess(ir.crate_name(), true)?;
        super::compliance::materialize(ir, inventory.crate_root(), assessment.findings())
    }
}
