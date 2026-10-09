//! `[amenable_ext]` / `[[amenable_ext.target]]`: per-target shadow-dep
//! registry coverage (`docs/planning/amenable-ext-targets-config.md`).

use std::collections::BTreeMap;

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
    /// Other crates the type resolver may read for this target, besides the
    /// target itself (`["chrono_tz"]` for `chrono`): where a path is
    /// followed and where implementors of a bound are looked for. A crate
    /// that is not listed stays opaque.
    #[serde(default)]
    resolve_crates: Vec<String>,
    /// Instantiations to track per generic type, as full argument tuples
    /// (`"chrono::Date" = [["chrono::Utc"]]`). An entry replaces what would
    /// otherwise be derived from rustdoc; an empty entry turns the
    /// instantiation rows off for that type.
    #[serde(default)]
    instantiations: BTreeMap<String, Vec<Vec<String>>>,
}

/// `[amenable_ext]`: the configured target list for shadow-dep registry
/// coverage.
///
/// No `[[amenable_ext.target]]` entries at all falls back to the
/// compiled-in default (`KNOWN_TARGETS`, jiff) -- see
/// `etiquettes::framework_ext::KNOWN_TARGETS`. Any entries listed here
/// *replace* that default rather than adding to it: to keep jiff alongside
/// a new target, list it explicitly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct AmenableExtConfig {
    #[serde(default, rename = "target")]
    #[getter(skip)]
    targets: Vec<AmenableExtTargetConfig>,
    /// Most instantiation rows derived from rustdoc for one generic type;
    /// over it, nothing is generated and a list is asked for.
    #[serde(default = "default_derive_cap")]
    #[getter(copy)]
    derive_cap: usize,
    /// Most chained type-alias expansions per name when resolving a type.
    #[serde(default = "default_alias_depth")]
    #[getter(copy)]
    alias_depth: usize,
    /// Most nodes in one resolved type.
    #[serde(default = "default_max_type_nodes")]
    #[getter(copy)]
    max_type_nodes: usize,
}

#[instrument(level = "trace")]
fn default_derive_cap() -> usize {
    16
}

#[instrument(level = "trace")]
fn default_alias_depth() -> usize {
    8
}

#[instrument(level = "trace")]
fn default_max_type_nodes() -> usize {
    64
}

impl Default for AmenableExtConfig {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            targets: Vec::new(),
            derive_cap: default_derive_cap(),
            alias_depth: default_alias_depth(),
            max_type_nodes: default_max_type_nodes(),
        }
    }
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

    /// The configuration of the target named `name`, if it has an entry.
    #[instrument(level = "trace", skip(self))]
    pub fn target(&self, name: &str) -> Option<&AmenableExtTargetConfig> {
        self.targets.iter().find(|target| target.name() == name)
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
