use crate::error::CordialResult;
use crate::loader::{CrateTarget, LoadView};
use crate::session::SessionView;

/// Reads raw material for analysis.
pub trait Loader: Send + Sync {
    /// Stable identifier for this hook.
    fn id(&self) -> &str;
    /// Read raw material for this crate and return a load view.
    fn load(&self, view: LoadContext<'_>) -> CordialResult<Box<dyn LoadView>>;
}

/// Shared inputs for [`Loader::load`].
///
/// Take the fields the loader needs; unused neighbors are not unused arguments.
#[derive(Clone, Copy)]
pub struct LoadContext<'a> {
    /// Session this hook is running in.
    pub session: &'a dyn SessionView,
    /// Crate being loaded or analyzed.
    pub target: &'a CrateTarget,
}
