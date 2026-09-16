use serde::{Deserialize, Serialize};
use tracing::instrument;

use super::default_true;

/// Visibility etiquette knobs.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    derive_new::new,
    derive_getters::Getters,
    derive_setters::Setters,
)]
#[setters(generate = false, prefix = "with_")]
pub struct VisibilityThresholds {
    /// If the crate has fewer than this many externally reachable `pub` names,
    /// no `pub mod` is allowed on a public path.
    #[serde(default = "default_max_crate_names_for_flat")]
    #[getter(copy)]
    max_crate_names_for_flat: usize,
    /// A visible module must contain at least this many leaf names.
    #[serde(default = "default_min_module_names")]
    #[getter(copy)]
    min_module_names: usize,
    /// Prefer a fat root over modules smaller than [`Self::min_module_names`].
    #[serde(default = "default_prefer_root")]
    #[new(value = "true")]
    #[getter(copy)]
    #[setters(generate)]
    prefer_root: bool,
    /// Run this etiquette (`true`) or skip it (`false`).
    #[serde(default = "default_true")]
    #[new(value = "true")]
    #[getter(copy)]
    enabled: bool,
    /// Per-crate module-path prefixes exempt from `VIS-MOD-THIN-001`
    /// specifically -- every other visibility rule (`VIS-CRATE-FLAT-001`,
    /// `VIS-MOD-MISMATCH-001`) still applies normally to these modules.
    /// Crate name to a list of paths relative to `crate` (`{ amenable_verus
    /// = ["gallery"] }` exempts `crate::gallery` and everything under it).
    /// A module matches when its own path equals the configured path or
    /// starts with `{path}::`. For a deliberately narrow, single-concept
    /// file (this project's own "one gallery investigation, one file"
    /// convention, or a `verus! {}`-derived codegen destination) that
    /// will never carry [`Self::min_module_names`] worth of its own
    /// names, on purpose -- not a documented per-finding exception (see
    /// `cordial exceptions add`), a structural statement that this rule's
    /// premise doesn't apply to the named subtree at all.
    #[serde(default)]
    #[new(default)]
    mod_thin_skip: std::collections::HashMap<String, Vec<String>>,
}

#[instrument(level = "debug")]
fn default_max_crate_names_for_flat() -> usize {
    50
}

#[instrument(level = "debug")]
fn default_min_module_names() -> usize {
    10
}

#[instrument(level = "debug")]
fn default_prefer_root() -> bool {
    true
}

impl Default for VisibilityThresholds {
    #[instrument(level = "debug")]
    fn default() -> Self {
        Self {
            max_crate_names_for_flat: default_max_crate_names_for_flat(),
            min_module_names: default_min_module_names(),
            prefer_root: default_prefer_root(),
            enabled: true,
            mod_thin_skip: std::collections::HashMap::new(),
        }
    }
}
