use crate::error::CordialResult;
use crate::ir::WorkspaceIr;
use crate::objects::Finding;
use crate::session::{RunFilter, SessionView};

/// Consumes workspace-scoped IR and emits cross-crate findings.
///
/// Workspace assessors handle rules that need multiple crate graphs or
/// workspace-level metadata before they can be judged.
pub trait WorkspaceAssessor: Send + Sync {
    /// Stable identifier used to deduplicate workspace assessor runs.
    fn id(&self) -> &str;
    /// Judge workspace IR and emit findings.
    fn assess(&self, view: WorkspaceAssessView<'_>) -> CordialResult<Vec<Box<dyn Finding>>>;
}

/// Shared inputs for [`WorkspaceAssessor::assess`].
///
/// Take the fields the assessor needs; unused neighbors are not unused
/// arguments.
#[derive(Clone, Copy, derive_getters::Getters, derive_new::new)]
pub struct WorkspaceAssessView<'a> {
    /// Workspace IR this assessor reads.
    #[getter(copy)]
    workspace: &'a WorkspaceIr,
    /// Session this hook is running in.
    #[getter(copy)]
    session: &'a dyn SessionView,
    /// Run filter for workspace-scoped assessment.
    #[getter(copy)]
    filter: &'a dyn RunFilter,
}
