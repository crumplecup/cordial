use crate::error::CordialResult;
use crate::ir::IrView;
use crate::objects::{Finding, Marker};
use crate::session::SessionView;

/// Shared inputs for [`Assessor::assess`].
///
/// Take the fields the assessor needs; unused neighbors are not unused
/// arguments.
#[derive(Clone, Copy)]
pub struct AssessView<'a> {
    /// Markers produced by probes in this session.
    pub markers: &'a [&'a dyn Marker],
    /// Crate IR graph for this hook invocation.
    pub ir: &'a dyn IrView,
    /// Session this hook is running in.
    pub session: &'a dyn SessionView,
}

/// Consumes markers and emits findings.
pub trait Assessor: Send + Sync {
    /// Stable identifier for this hook.
    fn id(&self) -> &str;
    /// Probe ids whose markers this assessor reads.
    fn consumes(&self) -> &[&str];
    /// Judge markers (or workspace IR) and emit findings.
    fn assess(&self, view: AssessView<'_>) -> CordialResult<Vec<Box<dyn Finding>>>;
}
