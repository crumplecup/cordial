use crate::error::CordialResult;
use crate::ir::IrView;
use crate::objects::{Finding, Marker};
use crate::session::SessionView;

/// Consumes markers and emits findings.
pub trait Assessor: Send + Sync {
    fn id(&self) -> &str;
    fn consumes(&self) -> &[&str];
    fn assess(
        &self,
        markers: &[&dyn Marker],
        ir: &dyn IrView,
        session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Finding>>>;
}
