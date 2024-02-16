//! The `bearing` module contains the [`Bearing`] struct, with methods for
//! constructing routes and handlers.
use crate::prelude::*;
#[cfg(feature = "route")]
use axum::Router;
#[cfg(feature = "route")]
use axum::routing::method_routing::MethodRouter;
#[cfg(feature = "sql")]
use sqlx::PgPool;

/// The `Bearing` struct gives direction to a guest ([`crate::guest::Guest`]).
#[cfg(feature = "route")]
#[cfg_attr(docsrs, doc(cfg(feature = "route")))]
#[cfg(feature = "sql")]
#[cfg_attr(docsrs, doc(cfg(feature = "sql")))]
#[derive(Debug, Clone)]
pub struct Bearing {
    /// The `cues` field indicates directions available to a guest ([`crate::guest::Guest`]).
    pub cues: Router<PgPool>,
}

impl Bearing {
    /// Creates a `Bearing` from a given state of [`Recall`].
    pub fn new(recall: Recall) -> Self {
        let cues = Router::new()
            .with_state(recall.book);
        Bearing { cues }
    }

    /// Adjust a `Bearing` by adding a `cue` to a method of `counsel`.  The `cue` field is a string
    /// representation of the route added to the [`Router`], while the `counsel` field specifies
    /// the associated handler(s).  Serves as a wrapper for [`Router::route`].
    pub fn adjust(mut self, cue: &str, counsel: MethodRouter<PgPool>) -> Self {
        self.cues = self.cues.route(cue, counsel);
        self
    }
}

impl From<Recall> for Bearing {
    fn from(recall: Recall) -> Self {
        Bearing::new(recall)
    }
}

impl From<&Recall> for Bearing {
    fn from(recall: &Recall) -> Self {
        Bearing::new(recall.clone())
    }
}
