use crate::error::CordialResult;
use crate::loader::{CrateTarget, LoadView};
use crate::session::SessionView;

/// Reads raw material for analysis.
pub trait Loader: Send + Sync {
    fn id(&self) -> &str;
    fn load(
        &self,
        session: &dyn SessionView,
        target: &CrateTarget,
    ) -> CordialResult<Box<dyn LoadView>>;
}
