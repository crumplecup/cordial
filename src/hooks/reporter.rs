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
    /// Findings produced by assessors in this session.
    pub findings: &'a [&'a dyn Finding],
    /// Crate IR graph for this hook invocation.
    pub ir: &'a dyn IrView,
    /// Session this hook is running in.
    pub session: &'a dyn SessionView,
}

/// Renders findings into artifacts.
pub trait Reporter: Send + Sync {
    /// Stable identifier for this hook.
    fn id(&self) -> &str;
    /// Render findings into artifacts.
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>>;
}
