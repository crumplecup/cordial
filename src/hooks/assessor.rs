use crate::error::CordialResult;
use crate::ir::IrView;
use crate::objects::{Finding, Marker};
use crate::session::SessionView;

use tracing::instrument;
/// Consumes markers and emits findings.
///
/// Assessors are where an etiquette makes judgments: rule id, disposition,
/// exception handling, and cross-marker joins all belong here.
pub trait Assessor: Send + Sync {
    /// Stable identifier used to deduplicate assessor runs.
    fn id(&self) -> &str;
    /// Probe ids whose markers this assessor reads.
    ///
    /// The session passes markers from matching producers to this assessor.
    fn consumes(&self) -> &[&str];
    /// Judge markers and emit findings.
    fn assess(&self, view: AssessView<'_>) -> CordialResult<Vec<Box<dyn Finding>>>;
}

/// Shared inputs for [`Assessor::assess`].
///
/// Take the fields the assessor needs; unused neighbors are not unused
/// arguments.
#[derive(Clone, Copy, derive_getters::Getters, derive_new::new)]
pub struct AssessView<'a> {
    /// Markers produced by probes in this session.
    #[getter(copy)]
    markers: &'a [&'a dyn Marker],
    /// Crate IR graph for this hook invocation.
    #[getter(copy)]
    ir: &'a dyn IrView,
    /// Session this hook is running in.
    #[getter(copy)]
    session: &'a dyn SessionView,
}

impl<'a> AssessView<'a> {
    /// Consume the view into its hook inputs.
    #[instrument(level = "debug", skip(self))]
    pub fn into_parts(self) -> (&'a dyn IrView, &'a [&'a dyn Marker], &'a dyn SessionView) {
        (self.ir, self.markers, self.session)
    }
}
