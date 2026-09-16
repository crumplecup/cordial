use serde::{Deserialize, Serialize};
use tracing::instrument;

use super::default_true;

/// cfg-scatter etiquette knobs.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    derive_new::new,
    derive_getters::Getters,
)]
pub struct CfgScatterThresholds {
    #[serde(default = "default_min_distinct_kinds")]
    #[getter(copy)]
    min_distinct_kinds: usize,
    #[serde(default = "default_min_occurrences")]
    #[getter(copy)]
    min_occurrences: usize,
    /// Run this etiquette (`true`) or skip it (`false`).
    #[serde(default = "default_true")]
    #[new(value = "true")]
    #[getter(copy)]
    enabled: bool,
}

#[instrument(level = "debug")]
fn default_min_distinct_kinds() -> usize {
    2
}

#[instrument(level = "debug")]
fn default_min_occurrences() -> usize {
    5
}

impl Default for CfgScatterThresholds {
    #[instrument(level = "debug")]
    fn default() -> Self {
        Self {
            min_distinct_kinds: default_min_distinct_kinds(),
            min_occurrences: default_min_occurrences(),
            enabled: true,
        }
    }
}

/// Cfg-hygiene etiquette knobs.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct CfgHygieneThresholds {
    /// Extra cfg names to always treat as declared, beyond rustc's ~32
    /// built-ins and Cargo's `test`/`feature`/`docsrs`  -- for a
    /// project-specific cfg this etiquette has no other way to discover
    /// (e.g. one injected by a custom xtask, not `build.rs`/`Cargo.toml`).
    #[serde(default)]
    extra_known_names: Vec<String>,
    /// Crate name -> the one verifier cfg name that crate's own source is
    /// allowed to gate on. `CFG-VERIFIER-MISMATCH-001` only checks crates
    /// listed here (each backend crate opts itself in), and only flags a
    /// *different* verifier's name found in that crate's own source --
    /// deliberately narrower than "flag any crate using a name it doesn't
    /// own", since a verifier's `--cfg` applies to its whole compiled
    /// dependency graph, so upstream crates legitimately shared across
    /// backends (e.g. a core types crate) reference more than one
    /// verifier's name on purpose. Empty by default: inert until a project
    /// configures it, e.g.
    /// `crate_verifier = { my_kani_crate = "kani", my_creusot_crate = "creusot" }`.
    #[serde(default)]
    crate_verifier: std::collections::HashMap<String, String>,
    /// Run this etiquette (`true`) or skip it (`false`).
    #[serde(default = "default_true")]
    #[new(value = "true")]
    #[getter(copy)]
    enabled: bool,
}

impl Default for CfgHygieneThresholds {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            extra_known_names: Vec::new(),
            crate_verifier: std::collections::HashMap::new(),
            enabled: true,
        }
    }
}
