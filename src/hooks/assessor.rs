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
    pub markers: &'a [&'a dyn Marker],
    pub ir: &'a dyn IrView,
    pub session: &'a dyn SessionView,
}

/// Consumes markers and emits findings.
pub trait Assessor: Send + Sync {
    fn id(&self) -> &str;
    fn consumes(&self) -> &[&str];
    fn assess(&self, view: AssessView<'_>) -> CordialResult<Vec<Box<dyn Finding>>>;
}
