use crate::error::CordialResult;
use crate::ir::{IrView, Query};
use crate::objects::Marker;
use crate::session::SessionView;

/// Shared inputs for [`Probe::probe`].
///
/// Take the fields the probe needs; unused neighbors are not unused arguments.
#[derive(Clone, Copy)]
pub struct ProbeView<'a> {
    pub ir: &'a dyn IrView,
    pub session: &'a dyn SessionView,
}

/// Walks the IR and emits markers.
pub trait Probe: Send + Sync {
    fn id(&self) -> &str;
    fn interests(&self) -> &dyn Query;
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>>;
}
