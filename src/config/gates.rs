use serde::{Deserialize, Serialize};
use tracing::instrument;

use super::default_true;

/// On/off gate for an etiquette that has no other `cordial.toml` knobs.
///
/// Default on. `[panics] enabled = false` skips that etiquette for the
/// project (see [`crate::config::CordialConfig::etiquette_enabled`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct EtiquetteGate {
    /// Run this etiquette (`true`) or skip it (`false`).
    #[serde(default = "default_true")]
    #[getter(copy)]
    enabled: bool,
}

impl Default for EtiquetteGate {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self { enabled: true }
    }
}
