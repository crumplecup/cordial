use crate::error::CordialResult;
use crate::hooks::{EnrichView, IrEnricher};

use tracing::instrument;
/// Rebuilds the crate `by_path` index from all `qualified_path` attrs.
#[derive(Debug, Default, Clone, Copy)]
pub struct PathIndexEnricher;

impl PathIndexEnricher {
    /// Stable identifier for `PathIndexEnricher`.
    pub const ID: &'static str = "path-index";
}

impl IrEnricher for PathIndexEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        1
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let ir = view.ir;

        ir.rebuild_path_index()?;
        Ok(())
    }
}
