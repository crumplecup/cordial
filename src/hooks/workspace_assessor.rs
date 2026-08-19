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
    pub workspace: &'a WorkspaceIr,
    pub session: &'a dyn SessionView,
    pub filter: &'a dyn RunFilter,
}

/// Consumes workspace-scoped IR and emits cross-crate findings.
pub trait WorkspaceAssessor: Send + Sync {
    fn id(&self) -> &str;
    fn assess(&self, view: WorkspaceAssessView<'_>) -> CordialResult<Vec<Box<dyn Finding>>>;
}
