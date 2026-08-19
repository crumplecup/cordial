use crate::error::CordialResult;
use crate::loader::{CrateTarget, LoadView};
use crate::session::SessionView;

/// Shared inputs for [`Loader::load`].
///
/// Take the fields the loader needs; unused neighbors are not unused arguments.
#[derive(Clone, Copy)]
pub struct LoadContext<'a> {
    pub session: &'a dyn SessionView,
    pub target: &'a CrateTarget,
}

/// Reads raw material for analysis.
pub trait Loader: Send + Sync {
    fn id(&self) -> &str;
    fn load(&self, view: LoadContext<'_>) -> CordialResult<Box<dyn LoadView>>;
}
