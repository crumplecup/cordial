use crate::error::CordialResult;
use crate::ir::WorkspaceIr;
use crate::objects::Finding;
use crate::session::{RunFilter, SessionView};

/// Shared inputs for [`WorkspaceAssessor::assess`].
///
/// Take the fields the assessor needs; unused neighbors are not unused
/// arguments.
#[derive(Clone, Copy)]
pub struct WorkspaceAssessView<'a> {
    /// Workspace IR this assessor reads.
    pub workspace: &'a WorkspaceIr,
    /// Session this hook is running in.
    pub session: &'a dyn SessionView,
    /// Run filter for workspace-scoped assessment.
    pub filter: &'a dyn RunFilter,
}

/// Consumes workspace-scoped IR and emits cross-crate findings.
pub trait WorkspaceAssessor: Send + Sync {
    /// Stable identifier for this hook.
    fn id(&self) -> &str;
    /// Judge markers (or workspace IR) and emit findings.
    fn assess(&self, view: WorkspaceAssessView<'_>) -> CordialResult<Vec<Box<dyn Finding>>>;
}
