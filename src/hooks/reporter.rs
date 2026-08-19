use crate::error::CordialResult;
use crate::ir::IrView;
use crate::objects::{Artifact, Finding};
use crate::session::SessionView;

/// Shared inputs for [`Reporter::render`].
///
/// Take the fields the reporter needs; unused neighbors are not unused
/// arguments.
#[derive(Clone, Copy)]
pub struct RenderView<'a> {
    pub findings: &'a [&'a dyn Finding],
    pub ir: &'a dyn IrView,
    pub session: &'a dyn SessionView,
}

/// Renders findings into artifacts.
pub trait Reporter: Send + Sync {
    fn id(&self) -> &str;
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>>;
}
