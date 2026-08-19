use crate::error::CordialResult;
use crate::hooks::IrEnricher;
use crate::ir::IrMut;
use crate::loader::LoadView;
use crate::session::SessionView;

use tracing::instrument;
/// Rebuilds the crate `by_path` index from all `qualified_path` attrs.
#[derive(Debug, Default, Clone, Copy)]
pub struct PathIndexEnricher;

impl PathIndexEnricher {
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

    #[instrument(level = "trace", skip(self, ir, _load, _session))]
    fn enrich(
        &self,
        ir: &mut dyn IrMut,
        _load: &dyn LoadView,
        _session: &dyn SessionView,
    ) -> CordialResult<()> {
        ir.rebuild_path_index()?;
        Ok(())
    }
}
