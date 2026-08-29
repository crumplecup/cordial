use crate::error::CordialResult;
use crate::ir::IrMut;
use crate::loader::{LoadView, SourceLoader};
use crate::session::SessionView;

/// Extends the IR with derived structure and attributes.
pub trait IrEnricher: Send + Sync {
    /// Stable identifier for this hook.
    fn id(&self) -> &str;

    /// Lower values run first among enrichers in one session.
    fn priority(&self) -> u8 {
        50
    }

    /// Loader whose [`LoadView`](crate::loader::LoadView) this enricher expects.
    fn required_loader(&self) -> &str {
        SourceLoader::ID
    }

    /// Mutate the IR with derived structure and attributes.
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()>;
}

/// Shared inputs for [`IrEnricher::enrich`].
///
/// Passed by value so the enricher can take `ir` mutably and ignore `load` or
/// `session` without unused-argument noise.
pub struct EnrichView<'a> {
    /// Crate IR graph for this hook invocation.
    pub ir: &'a mut dyn IrMut,
    /// Loader output this enricher may read.
    pub load: &'a dyn LoadView,
    /// Session this hook is running in.
    pub session: &'a dyn SessionView,
}
