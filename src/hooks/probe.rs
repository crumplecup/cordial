use crate::error::CordialResult;
use crate::ir::{IrView, Query};
use crate::objects::Marker;
use crate::session::SessionView;

/// Shared inputs for [`Probe::probe`].
///
/// Take the fields the probe needs; unused neighbors are not unused arguments.
#[derive(Clone, Copy)]
pub struct ProbeView<'a> {
    /// Crate IR graph for this hook invocation.
    pub ir: &'a dyn IrView,
    /// Session this hook is running in.
    pub session: &'a dyn SessionView,
}

/// Walks the IR and emits markers.
pub trait Probe: Send + Sync {
    /// Stable identifier for this hook.
    fn id(&self) -> &str;
    /// IR query describing which nodes this probe considers.
    fn interests(&self) -> &dyn Query;
    /// Walk the IR and emit markers.
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>>;
}
