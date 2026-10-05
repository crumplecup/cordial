//! `[amenable_ext]` / `[[amenable_ext.target]]`: per-target shadow-dep
//! registry coverage (`docs/planning/amenable-ext-targets-config.md`).

use serde::{Deserialize, Serialize};
use tracing::instrument;

use super::default_true;

/// One `[[amenable_ext.target]]` entry.
///
/// This `enabled` flag -- not the generic `[{etiquette-id}] enabled =
/// false` gate every other etiquette uses -- is the one mechanism that
/// turns a target off: dynamically-named etiquette ids (`amenable-ext-jiff`,
/// `amenable-ext-chrono`, ...) can't appear in
/// [`crate::config::CordialConfig::etiquette_enabled`]'s fixed match, since
/// that match is compiled once over a closed set of ids. Filtering here,
/// before a disabled target's etiquette is even built, also skips its
/// shadow-dep rustdoc build entirely rather than just skipping its probe
/// after the fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct AmenableExtTargetConfig {
    /// Upstream crate name (e.g. `"jiff"`).
    name: String,
    /// Build and run this target's coverage etiquette (`true`) or skip it
    /// entirely (`false`).
    #[serde(default = "default_true")]
    #[getter(copy)]
    enabled: bool,
}

/// `[amenable_ext]`: the configured target list for shadow-dep registry
/// coverage.
///
/// No `[[amenable_ext.target]]` entries at all falls back to the
/// compiled-in default (`KNOWN_TARGETS`, jiff) -- see
/// `etiquettes::framework_ext::KNOWN_TARGETS`. Any entries listed here
/// *replace* that default rather than adding to it: to keep jiff alongside
/// a new target, list it explicitly.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct AmenableExtConfig {
    #[serde(default, rename = "target")]
    #[getter(skip)]
    targets: Vec<AmenableExtTargetConfig>,
}

impl AmenableExtConfig {
    /// Enabled target names, in declaration order. Empty means "no
    /// `[[amenable_ext.target]]` section at all" -- callers fall back to
    /// `KNOWN_TARGETS` themselves, since an explicit empty list and "not
    /// configured" aren't distinguishable once deserialized.
    #[instrument(level = "trace", skip(self))]
    pub fn enabled_target_names(&self) -> Vec<&str> {
        self.targets
            .iter()
            .filter(|target| target.enabled())
            .map(|target| target.name())
            .map(String::as_str)
            .collect()
    }

    /// Whether any targets were configured at all (enabled or not) --
    /// distinguishes "no `[[amenable_ext.target]]` section" (fall back to
    /// `KNOWN_TARGETS`) from "configured, but every entry is disabled"
    /// (run none).
    #[instrument(level = "trace", skip(self))]
    pub fn has_configured_targets(&self) -> bool {
        !self.targets.is_empty()
    }
}
