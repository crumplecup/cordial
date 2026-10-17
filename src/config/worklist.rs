use serde::{Deserialize, Serialize};
use tracing::instrument;

/// Agent worklist limits loaded from `[worklist]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct WorklistSettings {
    /// Number of lists an acted-on item can survive before the agent asks for help.
    #[serde(default = "default_max_passes")]
    #[getter(copy)]
    max_passes: u32,
}

impl Default for WorklistSettings {
    #[instrument(level = "trace")]
    fn default() -> Self {
        Self {
            max_passes: default_max_passes(),
        }
    }
}

#[instrument(level = "trace")]
fn default_max_passes() -> u32 {
    3
}
