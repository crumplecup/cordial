//! Names for built-in and project-defined cost tiers.

use serde::{Deserialize, Serialize};
use tracing::instrument;

/// A named group of etiquettes selected for one run.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Tier(String);

impl Tier {
    /// The inexpensive built-in tier.
    pub const QUICK: &'static str = "quick";
    /// The tier with expensive cold setup.
    pub const BUILD: &'static str = "build";
    /// The tier with recurring expensive work.
    pub const DEEP: &'static str = "deep";

    /// Name a tier, including a project-defined one.
    #[instrument(level = "debug", skip(name))]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// The tier name.
    #[instrument(level = "trace", skip(self))]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for Tier {
    #[instrument(level = "debug")]
    fn default() -> Self {
        Self::new(Self::QUICK)
    }
}
