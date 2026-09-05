use crate::error::CordialResult;
use crate::ir::IrView;
use crate::objects::{Artifact, Finding};
use crate::session::SessionView;

/// Renders findings into artifacts.
///
/// Reporters format already-judged findings. They should not invent new
/// findings or silently change dispositions.
pub trait Reporter: Send + Sync {
    /// Stable identifier used to deduplicate reporter runs.
    fn id(&self) -> &str;
    /// Render findings into artifacts.
    ///
    /// Artifact filenames should be stable because users inspect them through
    /// `cordial view`, scripts, and diffs.
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>>;
}

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
