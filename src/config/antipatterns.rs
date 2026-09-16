use serde::{Deserialize, Serialize};
use tracing::instrument;

use super::default_true;

/// Antipattern etiquette knobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct AntipatternsConfig {
    /// Run this etiquette (`true`) or skip it (`false`).
    #[serde(default = "default_true")]
    #[getter(copy)]
    enabled: bool,
    /// Strategy knobs for `ANTIPATTERN-STRUCT-STATIC-REF-001`.
    #[serde(default)]
    static_refs: StaticRefPolicy,
}

impl Default for AntipatternsConfig {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            enabled: true,
            static_refs: StaticRefPolicy::default(),
        }
    }
}

/// Strategy policy for struct fields containing `&'static` references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct StaticRefPolicy {
    /// Preferred remediation strategy for `&'static str` fields.
    #[serde(default)]
    #[getter(copy)]
    strategy: StaticRefStrategy,
}

impl Default for StaticRefPolicy {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            strategy: StaticRefStrategy::default(),
        }
    }
}

/// Indicator for the static-reference canary strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaticRefStrategy {
    /// Prefer owned `String` data in structs.
    String,
    /// Prefer `Cow<'static, str>` where borrowed literals remain useful.
    Cow,
    /// Prefer static/const placement over runtime-owned struct fields.
    #[serde(alias = "static")]
    Const,
}

impl Default for StaticRefStrategy {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self::String
    }
}
