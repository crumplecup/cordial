use crate::error::CordialResult;
use crate::ir::{IrView, Query};
use crate::objects::Marker;
use crate::session::SessionView;

/// Walks the IR and emits markers.
pub trait Probe: Send + Sync {
    fn id(&self) -> &str;
    fn interests(&self) -> &dyn Query;
    fn probe(
        &self,
        ir: &dyn IrView,
        session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Marker>>>;
}
