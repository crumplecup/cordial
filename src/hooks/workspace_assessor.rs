use crate::error::CordialResult;
use crate::ir::WorkspaceIr;
use crate::objects::Finding;
use crate::session::{RunFilter, SessionView};

/// Consumes workspace-scoped IR and emits cross-crate findings.
pub trait WorkspaceAssessor: Send + Sync {
    fn id(&self) -> &str;
    fn assess(
        &self,
        workspace: &WorkspaceIr,
        session: &dyn SessionView,
        filter: &dyn RunFilter,
    ) -> CordialResult<Vec<Box<dyn Finding>>>;
}
