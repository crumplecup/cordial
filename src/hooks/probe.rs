use crate::error::CordialResult;
use crate::ir::{IrView, Query};
use crate::objects::Marker;
use crate::session::SessionView;

use tracing::instrument;
/// Walks the IR and emits markers.
///
/// Probes are observation producers. They should attach marker labels and
/// payload fields to relevant IR anchors, leaving severity and suppression to
/// assessors.
pub trait Probe: Send + Sync {
    /// Stable identifier used by markers and assessor routing.
    fn id(&self) -> &str;
    /// IR query describing which nodes this probe considers.
    ///
    /// The query is the coarse interest declaration; `probe` may still inspect
    /// neighboring facts before emitting a marker.
    fn interests(&self) -> &dyn Query;
    /// Walk the IR and emit markers.
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>>;
}

/// Shared inputs for [`Probe::probe`].
///
/// Take the fields the probe needs; unused neighbors are not unused arguments.
#[derive(Clone, Copy, derive_getters::Getters, derive_new::new)]
pub struct ProbeView<'a> {
    /// Crate IR graph for this hook invocation.
    #[getter(copy)]
    ir: &'a dyn IrView,
    /// Session this hook is running in.
    #[getter(copy)]
    session: &'a dyn SessionView,
}

impl<'a> ProbeView<'a> {
    /// Consume the view into its hook inputs.
    #[instrument(level = "debug", skip(self))]
    pub fn into_parts(self) -> (&'a dyn IrView, (), &'a dyn SessionView) {
        (self.ir, (), self.session)
    }
}
