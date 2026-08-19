use crate::error::CordialResult;
use crate::ir::IrMut;
use crate::loader::{LoadView, SourceLoader};
use crate::session::SessionView;

/// Shared inputs for [`IrEnricher::enrich`].
///
/// Passed by value so the enricher can take `ir` mutably and ignore `load` or
/// `session` without unused-argument noise.
pub struct EnrichView<'a> {
    pub ir: &'a mut dyn IrMut,
    pub load: &'a dyn LoadView,
    pub session: &'a dyn SessionView,
}

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

    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()>;
}
