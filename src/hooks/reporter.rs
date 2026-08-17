use crate::error::CordialResult;
use crate::ir::IrView;
use crate::objects::{Artifact, Finding};
use crate::session::SessionView;

/// Renders findings into artifacts.
pub trait Reporter: Send + Sync {
    fn id(&self) -> &str;
    fn render(
        &self,
        findings: &[&dyn Finding],
        ir: &dyn IrView,
        session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>>;
}
