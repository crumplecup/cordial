//! Project defaults and overrides for etiquette cost tiers.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::Tier;

/// Tier choices loaded from `[tiers]` in `cordial.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct TierConfig {
    /// Tiers selected when a run does not supply `--tier`.
    #[serde(default = "default_tiers")]
    default: Vec<String>,
    /// Per-etiquette assignments overriding built-ins and plugin declarations.
    #[serde(default)]
    assign: BTreeMap<String, String>,
}

#[instrument(level = "debug")]
fn default_tiers() -> Vec<String> {
    vec![Tier::QUICK.to_string(), Tier::BUILD.to_string()]
}

impl Default for TierConfig {
    #[instrument(level = "debug")]
    fn default() -> Self {
        Self {
            default: default_tiers(),
            assign: BTreeMap::new(),
        }
    }
}
