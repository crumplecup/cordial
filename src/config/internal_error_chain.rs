use serde::{Deserialize, Serialize};
use tracing::instrument;

use super::default_true;

/// Error architecture policy for `[internal_error_chain]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct InternalErrorChainThresholds {
    /// Maximum distinct literal call sites allowed for one Kind variant or
    /// native-source constructor. Both site count and distinct literal count
    /// must exceed this value. Zero flags the first matching call site.
    #[serde(default = "default_kind_fanout_max_call_sites")]
    #[getter(copy)]
    kind_fanout_max_call_sites: usize,
    /// Run this etiquette (`true`) or skip it (`false`).
    #[serde(default = "default_true")]
    #[getter(copy)]
    enabled: bool,
}

#[instrument(level = "trace")]
fn default_kind_fanout_max_call_sites() -> usize {
    3
}

impl Default for InternalErrorChainThresholds {
    #[instrument(level = "debug")]
    fn default() -> Self {
        Self {
            kind_fanout_max_call_sites: default_kind_fanout_max_call_sites(),
            enabled: true,
        }
    }
}
