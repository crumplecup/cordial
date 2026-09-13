use crate::error::CordialResult;
use crate::ir::IrMut;
use crate::loader::{LoadView, SourceLoader};
use crate::session::SessionView;

use tracing::instrument;
/// Extends the IR with derived structure and attributes.
///
/// Enrichers translate loader output into reusable graph facts. They should not
/// decide whether a fact is good or bad; that belongs in assessors.
pub trait IrEnricher: Send + Sync {
    /// Stable identifier used to deduplicate enricher runs.
    fn id(&self) -> &str;

    /// Lower values run first among enrichers in one session.
    ///
    /// Use this only for real fact dependencies, such as an index that another
    /// enricher reads.
    fn priority(&self) -> u8 {
        50
    }

    /// Loader whose [`LoadView`](crate::loader::LoadView) this enricher expects.
    fn required_loader(&self) -> &str {
        SourceLoader::ID
    }

    /// Mutate the IR with derived structure and attributes.
    ///
    /// Repeated runs over the same inputs should produce the same graph facts.
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()>;
}

/// Shared inputs for [`IrEnricher::enrich`].
///
/// Passed by value so the enricher can take `ir` mutably and ignore `load` or
/// `session` without unused-argument noise.
#[derive(derive_getters::Getters, derive_new::new)]
pub struct EnrichView<'a> {
    /// Crate IR graph for this hook invocation.
    #[getter(skip)]
    ir: &'a mut dyn IrMut,
    /// Loader output this enricher may read.
    #[getter(copy)]
    load: &'a dyn LoadView,
    /// Session this hook is running in.
    #[getter(copy)]
    session: &'a dyn SessionView,
}

impl<'a> EnrichView<'a> {
    /// Consume the view into its hook inputs.
    #[instrument(level = "debug", skip(self))]
    pub fn into_parts(self) -> (&'a mut dyn IrMut, &'a dyn LoadView, &'a dyn SessionView) {
        (self.ir, self.load, self.session)
    }

    /// Consume the view into its mutable IR input.
    #[instrument(level = "debug", skip(self))]
    pub fn ir(self) -> &'a mut dyn IrMut {
        self.ir
    }
}
