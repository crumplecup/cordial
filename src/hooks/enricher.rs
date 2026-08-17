use crate::error::CordialResult;
use crate::ir::IrMut;
use crate::loader::{LoadView, SourceLoader};
use crate::session::SessionView;

/// Extends the IR with derived structure and attributes.
pub trait IrEnricher: Send + Sync {
    fn id(&self) -> &str;

    /// Lower values run first among enrichers in one session.
    fn priority(&self) -> u8 {
        50
    }

    /// Loader whose [`LoadView`](crate::loader::LoadView) this enricher expects.
    fn required_loader(&self) -> &str {
        SourceLoader::ID
    }

    fn enrich(
        &self,
        ir: &mut dyn IrMut,
        load: &dyn LoadView,
        session: &dyn SessionView,
    ) -> CordialResult<()>;
}
