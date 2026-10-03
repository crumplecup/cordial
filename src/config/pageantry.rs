use serde::{Deserialize, Serialize};
use tracing::instrument;

use super::default_true;

/// Pageantry etiquette knobs: rule toggles and the barrel shim size limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct PageantryThresholds {
    /// Emit `PAGEANTRY-TRAIT-001` (trait after the leading trait block).
    #[serde(default = "default_true")]
    #[getter(copy)]
    trait_block: bool,
    /// Emit `PAGEANTRY-BARREL-001` (type or function body in `lib.rs` / `mod.rs`).
    #[serde(default = "default_true")]
    #[getter(copy)]
    barrel: bool,
    /// Emit `PAGEANTRY-BARREL-SHIM-001` (proc-macro entry point that is more than a shim).
    #[serde(default = "default_true")]
    #[getter(copy)]
    barrel_shim: bool,
    /// Longest body, in lines between the braces, a proc-macro entry point
    /// in `lib.rs` / `mod.rs` may have and still count as a delegating shim.
    #[serde(default = "default_max_shim_lines")]
    #[getter(copy)]
    max_shim_lines: usize,
    /// Run this etiquette (`true`) or skip it (`false`).
    #[serde(default = "default_true")]
    #[getter(copy)]
    enabled: bool,
}

#[instrument(level = "debug")]
fn default_max_shim_lines() -> usize {
    8
}

impl Default for PageantryThresholds {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            trait_block: true,
            barrel: true,
            barrel_shim: true,
            max_shim_lines: default_max_shim_lines(),
            enabled: true,
        }
    }
}

impl PageantryThresholds {
    /// Whether a pageantry rule should emit a finding.
    ///
    /// Unknown rule ids stay off.
    #[instrument(level = "debug", skip(self))]
    pub fn rule_enabled(&self, rule_id: &str) -> bool {
        match rule_id {
            "PAGEANTRY-TRAIT-001" => self.trait_block,
            "PAGEANTRY-BARREL-001" => self.barrel,
            "PAGEANTRY-BARREL-SHIM-001" => self.barrel_shim,
            _ => false,
        }
    }
}
