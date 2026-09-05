use crate::error::CordialResult;
use crate::ir::IrView;
use crate::objects::{Finding, Marker};
use crate::session::SessionView;

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
#[derive(Clone, Copy)]
pub struct AssessView<'a> {
    /// Markers produced by probes in this session.
    pub markers: &'a [&'a dyn Marker],
    /// Crate IR graph for this hook invocation.
    pub ir: &'a dyn IrView,
    /// Session this hook is running in.
    pub session: &'a dyn SessionView,
}
