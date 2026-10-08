use serde::{Deserialize, Serialize};
use tracing::instrument;

use super::default_true;

/// Crate-root `#![forbid(unsafe_code)]` / `#![warn(missing_docs)]` knobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct CrateAttrsThresholds {
    /// Require `#![forbid(unsafe_code)]` on each library root.
    #[serde(default = "default_true")]
    #[getter(copy)]
    forbid_unsafe: bool,
    /// Require `#![warn(missing_docs)]` (or deny/forbid) on each library root.
    #[serde(default = "default_true")]
    #[getter(copy)]
    missing_docs: bool,
    /// Package names that may omit `forbid(unsafe_code)` (an FFI crate, say).
    #[serde(default)]
    allow_unsafe: Vec<String>,
    /// Package names that may omit `warn(missing_docs)`.
    #[serde(default)]
    allow_missing_docs: Vec<String>,
    /// Run this etiquette (`true`) or skip it (`false`).
    #[serde(default = "default_true")]
    #[getter(copy)]
    enabled: bool,
}

impl Default for CrateAttrsThresholds {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            forbid_unsafe: true,
            missing_docs: true,
            allow_unsafe: Vec::new(),
            allow_missing_docs: Vec::new(),
            enabled: true,
        }
    }
}

impl CrateAttrsThresholds {
    /// Whether this package is exempt from `forbid(unsafe_code)`.
    #[instrument(level = "trace", skip(self))]
    pub fn skip_unsafe(&self, crate_name: &str) -> bool {
        !self.forbid_unsafe || self.allow_unsafe.iter().any(|name| name == crate_name)
    }

    /// Whether this package is exempt from `warn(missing_docs)`.
    #[instrument(level = "trace", skip(self))]
    pub fn skip_missing_docs(&self, crate_name: &str) -> bool {
        !self.missing_docs
            || self
                .allow_missing_docs
                .iter()
                .any(|name| name == crate_name)
    }
}

/// `cargo doc` / rustdoc-warning etiquette knobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct DocWarningsThresholds {
    /// Pass `--document-private-items` so private-item link lints fire.
    #[serde(default)]
    #[getter(copy)]
    document_private_items: bool,
    /// Pass `--all-features` (match CI that documents every feature).
    #[serde(default)]
    #[getter(copy)]
    all_features: bool,
    /// Package names that skip the `cargo doc` invocation.
    #[serde(default)]
    skip_crates: Vec<String>,
    /// Run this etiquette (`true`) or skip it (`false`).
    #[serde(default = "default_true")]
    #[getter(copy)]
    enabled: bool,
}

impl Default for DocWarningsThresholds {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            document_private_items: false,
            all_features: false,
            skip_crates: Vec::new(),
            enabled: true,
        }
    }
}

impl DocWarningsThresholds {
    /// Whether this package should not run `cargo doc`.
    #[instrument(level = "trace", skip(self))]
    pub fn skip(&self, crate_name: &str) -> bool {
        self.skip_crates.iter().any(|name| name == crate_name)
    }
}

/// `cargo hack` feature-powerset warning etiquette knobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct FeatureWarningsThresholds {
    /// Powerset depth (`cargo hack --depth`).
    #[serde(default = "default_feature_depth")]
    #[getter(copy)]
    depth: usize,
    /// Features left out of the powerset (`--exclude-features`).
    #[serde(default)]
    exclude_features: Vec<String>,
    /// Features always toggled together (`--group-features`).
    #[serde(default)]
    group_features: Vec<Vec<String>>,
    /// Also report warnings that fire in every combination (clippy sees these).
    #[serde(default)]
    #[getter(copy)]
    include_universal: bool,
    /// A gate naming more features than this is too wide to read: the advice
    /// becomes "introduce a private (`_`-prefixed) feature" instead.
    #[serde(default = "default_private_feature_threshold")]
    #[getter(copy)]
    private_feature_threshold: usize,
    /// Package names that skip the powerset run.
    #[serde(default)]
    skip_crates: Vec<String>,
    /// Run this etiquette (`true`) or skip it (`false`). Off by default: the
    /// powerset is many cold `cargo check`s.
    #[serde(default)]
    #[getter(copy)]
    enabled: bool,
}

#[instrument(level = "trace")]
fn default_feature_depth() -> usize {
    2
}

#[instrument(level = "trace")]
fn default_private_feature_threshold() -> usize {
    6
}

impl Default for FeatureWarningsThresholds {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            depth: default_feature_depth(),
            exclude_features: Vec::new(),
            group_features: Vec::new(),
            include_universal: false,
            private_feature_threshold: default_private_feature_threshold(),
            skip_crates: Vec::new(),
            enabled: false,
        }
    }
}

impl FeatureWarningsThresholds {
    /// Whether this package should not run the powerset.
    #[instrument(level = "trace", skip(self))]
    pub fn skip(&self, crate_name: &str) -> bool {
        self.skip_crates.iter().any(|name| name == crate_name)
    }
}

/// `cargo creusot prove` diagnostic etiquette knobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct CreusotDiagnosticsThresholds {
    /// Package names that skip the `cargo creusot prove` invocation.
    #[serde(default)]
    skip_crates: Vec<String>,
    /// Run this etiquette (`true`) or skip it (`false`).
    #[serde(default = "default_true")]
    #[getter(copy)]
    enabled: bool,
}

impl Default for CreusotDiagnosticsThresholds {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            skip_crates: Vec::new(),
            enabled: true,
        }
    }
}

impl CreusotDiagnosticsThresholds {
    /// Whether this package should not run `cargo creusot prove`.
    #[instrument(level = "trace", skip(self))]
    pub fn skip(&self, crate_name: &str) -> bool {
        self.skip_crates.iter().any(|name| name == crate_name)
    }
}

/// Dependency freshness etiquette knobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct DependencyFreshnessThresholds {
    /// Emit patch-drift findings.
    #[serde(default = "default_true")]
    #[getter(copy)]
    patch: bool,
    /// Emit minor-drift findings.
    #[serde(default = "default_true")]
    #[getter(copy)]
    minor: bool,
    /// Emit major-drift findings.
    #[serde(default = "default_true")]
    #[getter(copy)]
    major: bool,
    /// Emit exact manifest version pin findings.
    #[serde(default = "default_true")]
    #[getter(copy)]
    manifest_exact_pin: bool,
    /// Emit manifest upper-bound findings.
    #[serde(default = "default_true")]
    #[getter(copy)]
    manifest_upper_bound: bool,
    /// Emit manifest wildcard requirement findings.
    #[serde(default = "default_true")]
    #[getter(copy)]
    manifest_wildcard: bool,
    /// Emit manifest tilde requirement findings.
    #[serde(default = "default_true")]
    #[getter(copy)]
    manifest_tilde: bool,
    /// Emit findings for member manifests bypassing workspace dependency policy.
    #[serde(default = "default_true")]
    #[getter(copy)]
    manifest_workspace_bypass: bool,
    /// Run this etiquette (`true`) or skip it (`false`).
    #[serde(default = "default_true")]
    #[getter(copy)]
    enabled: bool,
}

impl Default for DependencyFreshnessThresholds {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            patch: true,
            minor: true,
            major: true,
            manifest_exact_pin: true,
            manifest_upper_bound: true,
            manifest_wildcard: true,
            manifest_tilde: true,
            manifest_workspace_bypass: true,
            enabled: true,
        }
    }
}

impl DependencyFreshnessThresholds {
    /// Whether a dependency freshness rule should emit a finding.
    #[instrument(level = "debug", skip(self))]
    pub fn rule_enabled(&self, rule_id: &str) -> bool {
        match rule_id {
            "DEPENDENCY-FRESHNESS-PATCH" => self.patch,
            "DEPENDENCY-FRESHNESS-MINOR" => self.minor,
            "DEPENDENCY-FRESHNESS-MAJOR" => self.major,
            "DEPENDENCY-FRESHNESS-MANIFEST-EXACT-PIN" => self.manifest_exact_pin,
            "DEPENDENCY-FRESHNESS-MANIFEST-UPPER-BOUND" => self.manifest_upper_bound,
            "DEPENDENCY-FRESHNESS-MANIFEST-WILDCARD" => self.manifest_wildcard,
            "DEPENDENCY-FRESHNESS-MANIFEST-TILDE" => self.manifest_tilde,
            "DEPENDENCY-FRESHNESS-MANIFEST-WORKSPACE-BYPASS" => self.manifest_workspace_bypass,
            _ => false,
        }
    }
}

/// Derive-pattern etiquette knobs.
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
pub struct DerivesThresholds {
    /// `fn new` with more arguments than this should use a builder.
    /// At or below this count, a trivial `new` may use `derive_new` instead.
    #[serde(default = "default_max_constructor_args")]
    #[getter(copy)]
    max_constructor_args: usize,
    /// Inherent `mut self` fluent setters at or above this count mean the
    /// type is a hand-rolled builder and should `#[derive(Builder)]`.
    #[serde(default = "default_min_fluent_setters")]
    #[getter(copy)]
    min_fluent_setters: usize,
    /// Run this etiquette (`true`) or skip it (`false`).
    #[serde(default = "default_true")]
    #[new(value = "true")]
    #[getter(copy)]
    enabled: bool,
}

#[instrument(level = "debug")]
fn default_max_constructor_args() -> usize {
    3
}

#[instrument(level = "debug")]
fn default_min_fluent_setters() -> usize {
    2
}

impl Default for DerivesThresholds {
    #[instrument(level = "debug")]
    fn default() -> Self {
        Self {
            max_constructor_args: default_max_constructor_args(),
            min_fluent_setters: default_min_fluent_setters(),
            enabled: true,
        }
    }
}
