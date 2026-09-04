//! Layered `cordial.toml` — the canonical home for etiquette thresholds.
//!
//! Sources, later winning:
//! 1. [`CordialConfig::default`] (graceful fallback if no file exists)
//! 2. `{store_home}/cordial.toml` (`~/.cordial` by default)
//! 3. `{workspace}/cordial.toml`

use std::path::Path;

use config::{Config, File, FileFormat};
use serde::{Deserialize, Serialize};

use crate::session::SessionView;

use tracing::instrument;
/// All etiquette knobs loaded from `cordial.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct CordialConfig {
    #[serde(default)]
    visibility: VisibilityThresholds,
    #[serde(default)]
    modularity: ModularityThresholds,
    #[serde(default)]
    cfg_scatter: CfgScatterThresholds,
    #[serde(default)]
    cfg_hygiene: CfgHygieneThresholds,
    #[serde(default)]
    crate_attrs: CrateAttrsThresholds,
    #[serde(default)]
    doc_warnings: DocWarningsThresholds,
    #[serde(default)]
    tracing: TracingThresholds,
    #[serde(default)]
    derives: DerivesThresholds,
    #[serde(default)]
    panics: EtiquetteGate,
    #[serde(default)]
    allows: EtiquetteGate,
    #[serde(default)]
    error_sites: EtiquetteGate,
    #[serde(default)]
    error_chain: EtiquetteGate,
    #[serde(default)]
    internal_error_chain: EtiquetteGate,
    #[serde(default)]
    foreign_error_types: EtiquetteGate,
    #[serde(default)]
    foreign_error_attenuation: EtiquetteGate,
    #[serde(default)]
    antipatterns: EtiquetteGate,
    #[serde(default)]
    cli_layout: EtiquetteGate,
    #[serde(default)]
    glob_imports: EtiquetteGate,
    #[serde(default)]
    inline_tests: EtiquetteGate,
    #[serde(default)]
    verus_warnings: EtiquetteGate,
    #[serde(default)]
    proof_patterns: EtiquetteGate,
    #[serde(default)]
    pageantry: EtiquetteGate,
    #[serde(rename = "impl-coverage", default)]
    impl_coverage: EtiquetteGate,
    #[serde(default)]
    trenchcoat: EtiquetteGate,
    #[serde(default)]
    shadow: EtiquetteGate,
    #[serde(rename = "homecoming-std", default)]
    homecoming_std: EtiquetteGate,
    #[serde(rename = "amenable-std", default)]
    amenable_std: EtiquetteGate,
}

/// On/off gate for an etiquette that has no other `cordial.toml` knobs.
///
/// Default on. `[panics] enabled = false` skips that etiquette for the
/// project (see [`CordialConfig::etiquette_enabled`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct EtiquetteGate {
    /// Run this etiquette (`true`) or skip it (`false`).
    #[serde(default = "default_true")]
    #[getter(copy)]
    enabled: bool,
}

impl Default for EtiquetteGate {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl CordialConfig {
    /// Whether this etiquette should run for the project.
    ///
    /// Unknown ids (custom plugins) stay on. Built-ins read
    /// `[<id>] enabled` from `cordial.toml` (default `true`).
    #[instrument(level = "debug", skip(self))]
    pub fn etiquette_enabled(&self, id: &str) -> bool {
        match id {
            "visibility" => self.visibility.enabled,
            "modularity" => self.modularity.enabled,
            "cfg_scatter" => self.cfg_scatter.enabled,
            "cfg_hygiene" => self.cfg_hygiene.enabled,
            "crate_attrs" => self.crate_attrs.enabled,
            "doc_warnings" => self.doc_warnings.enabled,
            "tracing" => self.tracing.enabled,
            "derives" => self.derives.enabled,
            "panics" => self.panics.enabled,
            "allows" => self.allows.enabled,
            "error_sites" => self.error_sites.enabled,
            "error_chain" => self.error_chain.enabled,
            "internal_error_chain" => self.internal_error_chain.enabled,
            "foreign_error_types" => self.foreign_error_types.enabled,
            "foreign_error_attenuation" => self.foreign_error_attenuation.enabled,
            "antipatterns" => self.antipatterns.enabled,
            "cli_layout" => self.cli_layout.enabled,
            "glob_imports" => self.glob_imports.enabled,
            "inline_tests" => self.inline_tests.enabled,
            "verus_warnings" => self.verus_warnings.enabled,
            "proof_patterns" => self.proof_patterns.enabled,
            "pageantry" => self.pageantry.enabled,
            "impl-coverage" => self.impl_coverage.enabled,
            "trenchcoat" => self.trenchcoat.enabled,
            "shadow" => self.shadow.enabled,
            "homecoming-std" => self.homecoming_std.enabled,
            "amenable-std" => self.amenable_std.enabled,
            _ => true,
        }
    }
}

/// Visibility etiquette knobs.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
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

impl VisibilityThresholds {
    /// Return a copy with `prefer_root` set.
    #[instrument(level = "trace", skip(self))]
    pub fn with_prefer_root(mut self, prefer_root: bool) -> Self {
        self.prefer_root = prefer_root;
        self
    }
}

impl Default for VisibilityThresholds {
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

/// Modularity etiquette knobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct ModularityThresholds {
    /// File inventory floor, and the *upper-tail* MODULE-SIZE checklist
    /// floor. A large-side 2σ module below this many lines stays
    /// inventory-only. The lower tail is not gated by this number.
    #[serde(default = "default_file_inventory_min_lines")]
    #[getter(copy)]
    file_inventory_min_lines: u32,
    /// Track function and method bodies at least this long (CSV inventory).
    #[serde(default = "default_function_inventory_min_lines")]
    #[getter(copy)]
    function_inventory_min_lines: u32,
    /// On files already at the file-inventory floor, name bodies at least this
    /// long as extract-helpers on the hotspot. Does not lower CSV inventory.
    #[serde(default = "default_function_hotspot_min_lines")]
    #[getter(copy)]
    function_hotspot_min_lines: u32,
    #[serde(default = "default_file_checklist_min_lines")]
    #[getter(copy)]
    file_checklist_min_lines: u32,
    /// Flag a function or method body this long as "split this body".
    #[serde(default = "default_function_checklist_min_lines")]
    #[getter(copy)]
    function_checklist_min_lines: u32,
    /// Warn when a file defines more `struct`/`enum`/`union`/`trait` items than this.
    #[serde(default = "default_max_types_per_file")]
    #[getter(copy)]
    max_types_per_file: u32,
    /// Flag a module when its size is more than this many sample standard
    /// deviations from the crate's mean module size.
    #[serde(default = "default_module_size_sigma")]
    #[getter(copy)]
    module_size_sigma: u32,
    /// When true, only the upper tail (`z > σ`) is a MODULE-SIZE checklist
    /// item. The lower tail stays in the sample and the summary; it does
    /// not become an action item. Default is two-tailed.
    #[serde(default)]
    #[getter(copy)]
    module_size_ignore_lower_tail: bool,
    /// Exclude modules smaller than this from the 2σ sample. `0` includes all.
    /// This is a sample filter, not a checklist floor — do not use it to
    /// silence the lower tail.
    #[serde(default = "default_min_module_lines")]
    #[getter(copy)]
    min_module_lines: u32,
    /// Checklist a parent that kept at least this percent of its subtree
    /// (`own * 100 / subtree`).
    #[serde(default = "default_top_heavy_min_percent")]
    #[getter(copy)]
    top_heavy_min_percent: u32,
    /// Checklist when one child holds at least this percent of its siblings'
    /// combined subtree (siblings below `hierarchy_min_lines` are ignored).
    #[serde(default = "default_lopsided_min_percent")]
    #[getter(copy)]
    lopsided_min_percent: u32,
    /// Ignore hierarchy hits whose parent own-lines (top-heavy), dominant
    /// subtree (lopsided), or passthrough subtree (collapse) is smaller than
    /// this.
    #[serde(default = "default_hierarchy_min_lines")]
    #[getter(copy)]
    hierarchy_min_lines: u32,
    /// Crate-relative file paths (or path prefixes, for a whole directory
    /// of generated files) exempt from the file-size and module-size LOC
    /// checks (`MODULARITY-FILE`, `MODULARITY-MODULE-SIZE`). There is no
    /// reliable way to detect "this file is generated" from the source
    /// alone, so known generated targets (codegen output, derived witness
    /// modules, ...) are named here instead. Does not exempt
    /// `MODULARITY-TYPES-PER-FILE` or `MODULARITY-FUNCTION` -- those are
    /// per-type and per-function signals, not the file's own LOC count.
    /// Replacing this list in `cordial.toml` replaces the default (empty),
    /// it does not union with it.
    #[serde(default)]
    generated_files: Vec<String>,
    /// Run this etiquette (`true`) or skip it (`false`).
    #[serde(default = "default_true")]
    #[getter(copy)]
    enabled: bool,
}

#[instrument(level = "debug")]
fn default_file_inventory_min_lines() -> u32 {
    500
}

#[instrument(level = "debug")]
fn default_function_inventory_min_lines() -> u32 {
    150
}

#[instrument(level = "debug")]
fn default_function_hotspot_min_lines() -> u32 {
    80
}

#[instrument(level = "debug")]
fn default_file_checklist_min_lines() -> u32 {
    1000
}

#[instrument(level = "debug")]
fn default_function_checklist_min_lines() -> u32 {
    200
}

#[instrument(level = "debug")]
fn default_max_types_per_file() -> u32 {
    10
}

#[instrument(level = "debug")]
fn default_module_size_sigma() -> u32 {
    2
}

#[instrument(level = "debug")]
fn default_min_module_lines() -> u32 {
    0
}

#[instrument(level = "debug")]
fn default_top_heavy_min_percent() -> u32 {
    50
}

#[instrument(level = "debug")]
fn default_lopsided_min_percent() -> u32 {
    75
}

#[instrument(level = "debug")]
fn default_hierarchy_min_lines() -> u32 {
    150
}

impl Default for ModularityThresholds {
    fn default() -> Self {
        Self {
            file_inventory_min_lines: default_file_inventory_min_lines(),
            function_inventory_min_lines: default_function_inventory_min_lines(),
            function_hotspot_min_lines: default_function_hotspot_min_lines(),
            file_checklist_min_lines: default_file_checklist_min_lines(),
            function_checklist_min_lines: default_function_checklist_min_lines(),
            max_types_per_file: default_max_types_per_file(),
            module_size_sigma: default_module_size_sigma(),
            module_size_ignore_lower_tail: false,
            min_module_lines: default_min_module_lines(),
            top_heavy_min_percent: default_top_heavy_min_percent(),
            lopsided_min_percent: default_lopsided_min_percent(),
            hierarchy_min_lines: default_hierarchy_min_lines(),
            generated_files: Vec::new(),
            enabled: true,
        }
    }
}

impl ModularityThresholds {
    /// Return a copy with `module_size_ignore_lower_tail` set.
    #[instrument(level = "debug")]
    pub fn with_module_size_ignore_lower_tail(self, ignore: bool) -> Self {
        Self {
            module_size_ignore_lower_tail: ignore,
            ..self
        }
    }

    /// Return a copy with `file_inventory_min_lines` set.
    #[instrument(level = "debug")]
    pub fn with_file_inventory_min_lines(self, value: u32) -> Self {
        Self {
            file_inventory_min_lines: value,
            ..self
        }
    }

    /// Return a copy with `function_inventory_min_lines` set.
    #[instrument(level = "debug")]
    pub fn with_function_inventory_min_lines(self, value: u32) -> Self {
        Self {
            function_inventory_min_lines: value,
            ..self
        }
    }

    /// Return a copy with `function_hotspot_min_lines` set.
    #[instrument(level = "debug")]
    pub fn with_function_hotspot_min_lines(self, value: u32) -> Self {
        Self {
            function_hotspot_min_lines: value,
            ..self
        }
    }

    /// Return a copy with `file_checklist_min_lines` set.
    #[instrument(level = "debug")]
    pub fn with_file_checklist_min_lines(self, value: u32) -> Self {
        Self {
            file_checklist_min_lines: value,
            ..self
        }
    }

    /// Return a copy with `function_checklist_min_lines` set.
    #[instrument(level = "debug")]
    pub fn with_function_checklist_min_lines(self, value: u32) -> Self {
        Self {
            function_checklist_min_lines: value,
            ..self
        }
    }

    /// Return a copy with `max_types_per_file` set.
    #[instrument(level = "debug")]
    pub fn with_max_types_per_file(self, value: u32) -> Self {
        Self {
            max_types_per_file: value,
            ..self
        }
    }

    /// Return a copy with `lopsided_min_percent` set.
    #[instrument(level = "debug")]
    pub fn with_lopsided_min_percent(self, value: u32) -> Self {
        Self {
            lopsided_min_percent: value,
            ..self
        }
    }

    /// Return a copy with `hierarchy_min_lines` set.
    #[instrument(level = "debug")]
    pub fn with_hierarchy_min_lines(self, value: u32) -> Self {
        Self {
            hierarchy_min_lines: value,
            ..self
        }
    }

    /// Return a copy with `generated_files` set.
    #[instrument(level = "debug", skip(value))]
    pub fn with_generated_files(self, value: Vec<String>) -> Self {
        Self {
            generated_files: value,
            ..self
        }
    }

    /// Whether `numerator / denominator` is at least `percent`.
    #[instrument(level = "debug")]
    pub fn ratio_meets(numerator: u32, denominator: u32, percent: u32) -> bool {
        denominator > 0 && u64::from(numerator) * 100 >= u64::from(denominator) * u64::from(percent)
    }

    /// Whether own-file lines vs subtree lines exceed the top-heavy threshold.
    #[instrument(level = "trace", skip(self))]
    pub fn is_top_heavy_hit(&self, own_lines: u32, subtree_lines: u32) -> bool {
        own_lines >= self.hierarchy_min_lines
            && Self::ratio_meets(own_lines, subtree_lines, self.top_heavy_min_percent)
    }

    /// Whether the largest child vs sibling total exceeds the lopsided threshold.
    #[instrument(level = "trace", skip(self))]
    pub fn is_lopsided_hit(&self, largest_subtree: u32, sibling_total: u32) -> bool {
        largest_subtree >= self.hierarchy_min_lines
            && Self::ratio_meets(largest_subtree, sibling_total, self.lopsided_min_percent)
    }

    /// Checklist a unary child directory whose subtree is large enough to
    /// bother collapsing (the extra hop is the bug; there is no percent knob).
    #[instrument(level = "trace", skip(self))]
    pub fn is_collapse_hit(&self, passthrough_subtree: u32) -> bool {
        passthrough_subtree >= self.hierarchy_min_lines
    }

    /// MODULE-SIZE checklist from a signed z-score.
    ///
    /// Upper tail (`z > σ`): checklist only when `lines` is at least
    /// [`Self::file_inventory_min_lines`]. The file floor does not apply
    /// to the lower tail.
    /// Lower tail (`z < -σ`): checklist unless
    /// [`Self::module_size_ignore_lower_tail`] is set.
    #[instrument(level = "trace", skip(self))]
    pub fn is_module_size_checklist(&self, lines: u32, zscore: Option<f64>) -> bool {
        let Some(zscore) = zscore else {
            return false;
        };
        let sigma = f64::from(self.module_size_sigma);
        if zscore > sigma {
            return lines >= self.file_inventory_min_lines;
        }
        if zscore < -sigma {
            return !self.module_size_ignore_lower_tail;
        }
        false
    }

    /// Function-body floor used while scanning one file.
    ///
    /// Inventory-sized files also record shorter bodies so too-long hotspots
    /// can name extract-helper candidates. CSV inventory stays at
    /// [`Self::function_inventory_min_lines`].
    #[instrument(level = "debug", skip(self))]
    pub fn function_scan_min_lines(&self, file_lines: u32) -> u32 {
        if file_lines >= self.file_inventory_min_lines {
            self.function_hotspot_min_lines
                .min(self.function_inventory_min_lines)
        } else {
            self.function_inventory_min_lines
        }
    }
}

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
    #[instrument(level = "debug", skip(self))]
    pub fn skip_unsafe(&self, crate_name: &str) -> bool {
        !self.forbid_unsafe || self.allow_unsafe.iter().any(|name| name == crate_name)
    }

    /// Whether this package is exempt from `warn(missing_docs)`.
    #[instrument(level = "debug", skip(self))]
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
    #[instrument(level = "debug", skip(self))]
    pub fn skip(&self, crate_name: &str) -> bool {
        self.skip_crates.iter().any(|name| name == crate_name)
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
    fn default() -> Self {
        Self {
            max_constructor_args: default_max_constructor_args(),
            min_fluent_setters: default_min_fluent_setters(),
            enabled: true,
        }
    }
}

/// Tracing etiquette knobs.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct TracingThresholds {
    /// Extra parameter names unioned with the built-in skip list.
    #[serde(default)]
    extra_skip: Vec<String>,
    /// Crate name -> cfg name. `--apply` wraps `#[instrument(..)]` as
    /// `#[cfg_attr(not(#cfg), instrument(..))]` for every function in
    /// this crate (and any crate that transitively depends on it, since
    /// e.g. `cargo kani`'s `--cfg kani` applies to the whole dependency
    /// graph it compiles, not just the top-level target crate) --
    /// real precedent: a bare `#[instrument]` on any function reachable
    /// from a `#[kani::proof]` harness causes real CBMC symbolic-
    /// closure-capture timeouts (confirmed via a real gallery
    /// experiment in a sibling project's own prior art), not just risks
    /// one.
    #[serde(default)]
    apply_gate_crates: std::collections::HashMap<String, String>,
    /// Crate names `--apply` never writes `#[instrument]` into at all,
    /// leaving the checklist item open -- for a crate whose real
    /// toolchain either can't resolve the `tracing` crate at all (a
    /// bare-compiler invocation that never reads `Cargo.toml`, real
    /// precedent: `verus --crate-type=lib`), or hard-fails compilation
    /// on `#[instrument]`'s own expansion (real precedent: Creusot's
    /// translator can't handle the static `DefaultCallsite` reference
    /// `tracing::span!` embeds, confirmed via a real `cargo creusot`
    /// run, not assumed from a milder "generated companions only" read
    /// of the failure). Unlike `apply_gate_crates`, this does **not**
    /// propagate through the ordinary dependency graph -- a translator
    /// that only sweeps a crate's own local items has no reason to
    /// touch an ordinary dependency's source at all (real precedent:
    /// `creusot-rustc`) -- but it does propagate through a `#[path]`
    /// splice, since that copies the physical file's real content into
    /// the splicing crate's own compilation unit.
    #[serde(default)]
    apply_skip_crates: Vec<String>,
    /// Subscriber-init policy knobs. Each defaults **on**.
    #[serde(default)]
    #[new(default)]
    subscriber: TracingSubscriberPolicy,
    /// Binary error-boundary policy knobs. Defaults **on**.
    #[serde(default)]
    #[new(default)]
    boundary: TracingBoundaryPolicy,
    /// Leftover-stdio filter. Each macro defaults **on**.
    #[serde(default)]
    #[new(default)]
    stdio: TracingStdioPolicy,
    /// Run this etiquette (`true`) or skip it (`false`).
    #[serde(default = "default_true")]
    #[new(value = "true")]
    #[getter(copy)]
    enabled: bool,
}

/// Whether each tracing-subscriber init rule is armed.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct TracingSubscriberPolicy {
    /// `fn main` in a binary must call the crate's init helper.
    #[serde(default = "default_true")]
    #[getter(copy)]
    init_in_main: bool,
    /// Each `#[test]` under `tests/` must call the same helper.
    #[serde(default = "default_true")]
    #[getter(copy)]
    init_in_tests: bool,
    /// The function that builds/installs the subscriber lives in the library.
    #[serde(default = "default_true")]
    #[getter(copy)]
    helper_in_lib: bool,
    /// That helper reads `RUST_LOG` and has a fallback (not `from_default_env()` alone).
    #[serde(default = "default_true")]
    #[getter(copy)]
    rust_log_fallback: bool,
    /// That helper uses `try_init()` or wraps `init()` in `Once` / `OnceLock`.
    #[serde(default = "default_true")]
    #[getter(copy)]
    idempotent: bool,
    /// Fully-qualified paths (e.g. `amenable_core::init_tracing`) of a
    /// shared helper defined in one crate and called from a sibling
    /// crate's `main`/`#[test]` -- a real, common shape in a multi-crate
    /// workspace that a single-crate scan can never verify on its own
    /// (the helper's *defining* crate is scanned separately, and its own
    /// body is checked there via `helper_in_lib`/`rust_log_fallback`/
    /// `idempotent`). A call matching one of these is trusted as a
    /// complete, compliant install; empty by default (inert until a
    /// project actually has a cross-crate helper).
    #[serde(default)]
    known_helper_paths: Vec<String>,
}

#[instrument(level = "debug")]
fn default_true() -> bool {
    true
}

impl Default for TracingSubscriberPolicy {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            init_in_main: true,
            init_in_tests: true,
            helper_in_lib: true,
            rust_log_fallback: true,
            idempotent: true,
            known_helper_paths: Vec::new(),
        }
    }
}

/// Whether the binary error-boundary rule is armed.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct TracingBoundaryPolicy {
    /// A fallible `fn main` in a binary must convert its error to a
    /// tracing warn/error emission (via `#[instrument(err(...))]` or an
    /// explicit `tracing::warn!`/`error!` on the error path) before the
    /// process boundary, instead of letting it bubble up and crash.
    #[serde(default = "default_true")]
    #[getter(copy)]
    main_reports_errors: bool,
    /// Fully-qualified paths (e.g. `amenable_core::run_and_report`) of a
    /// shared dispatch helper defined in one crate and called from a
    /// sibling crate's `main` — trusted as already reporting its own
    /// errors, the same way `[tracing.subscriber] known_helper_paths`
    /// trusts a cross-crate init helper. Empty by default.
    #[serde(default)]
    known_helper_paths: Vec<String>,
}

impl Default for TracingBoundaryPolicy {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            main_reports_errors: true,
            known_helper_paths: Vec::new(),
        }
    }
}

/// Whether each leftover-stdio macro is armed, plus folder / cargo skips.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct TracingStdioPolicy {
    /// Flag leftover `println!` (including `std::println!`).
    #[serde(default = "default_true")]
    #[getter(copy)]
    println: bool,
    /// Flag leftover `eprintln!`.
    #[serde(default = "default_true")]
    #[getter(copy)]
    eprintln: bool,
    /// Flag leftover `print!`.
    #[serde(default = "default_true")]
    #[getter(copy)]
    print: bool,
    /// Flag leftover `eprint!`.
    #[serde(default = "default_true")]
    #[getter(copy)]
    eprint: bool,
    /// Flag leftover `dbg!`.
    #[serde(default = "default_true")]
    #[getter(copy)]
    dbg: bool,
    /// Skip first-string `cargo:` / `cargo::` build-script protocol.
    #[serde(default = "default_true")]
    #[getter(copy)]
    skip_cargo_protocol: bool,
    /// Crate-relative folder prefixes to skip (`tests/fixtures`, `src/generated`).
    /// Replacing this list in `cordial.toml` replaces the defaults, it does
    /// not union with them.
    #[serde(default = "default_stdio_skip_folders")]
    skip_folders: Vec<String>,
}

#[instrument(level = "debug")]
fn default_stdio_skip_folders() -> Vec<String> {
    vec!["tests/fixtures".to_string(), "tests/parity".to_string()]
}

impl TracingStdioPolicy {
    /// Whether `file` lives under a configured skip folder.
    #[instrument(level = "debug", skip(self, file, crate_root), ret)]
    pub fn skips_file(&self, file: &Path, crate_root: &Path) -> bool {
        let rel = file.strip_prefix(crate_root).unwrap_or(file);
        self.skip_folders.iter().any(|folder| {
            let prefix = Path::new(folder);
            rel == prefix || rel.starts_with(prefix)
        })
    }
}

impl Default for TracingStdioPolicy {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            println: true,
            eprintln: true,
            print: true,
            eprint: true,
            dbg: true,
            skip_cargo_protocol: true,
            skip_folders: default_stdio_skip_folders(),
        }
    }
}

impl Default for TracingThresholds {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            extra_skip: Vec::new(),
            apply_gate_crates: std::collections::HashMap::new(),
            apply_skip_crates: Vec::new(),
            subscriber: TracingSubscriberPolicy::default(),
            boundary: TracingBoundaryPolicy::default(),
            stdio: TracingStdioPolicy::default(),
            enabled: true,
        }
    }
}

/// Load `cordial.toml` from the workspace and `{store_home}/cordial.toml`,
/// layered over [`CordialConfig::default`]. Workspace wins. Missing or
/// unreadable files fall back to `Default` instead of failing the run.
#[instrument(level = "info")]
pub fn load_cordial_config(workspace_root: &Path, store_home: &Path) -> CordialConfig {
    let mut builder = Config::builder();
    if let Ok(defaults) = Config::try_from(&CordialConfig::default()) {
        builder = builder.add_source(defaults);
    }
    builder = add_optional_toml(builder, &store_home.join("cordial.toml"));
    builder = add_optional_toml(builder, &workspace_root.join("cordial.toml"));
    builder
        .build()
        .and_then(|settings| settings.try_deserialize())
        .unwrap_or_default()
}

/// Same as [`load_cordial_config`] using the session's project root and store home.
#[instrument(level = "info", skip(session))]
pub fn load_session_config(session: &dyn SessionView) -> CordialConfig {
    load_cordial_config(session.project_root(), session.store_home())
}

/// Convenience for the visibility etiquette.
#[instrument(level = "info")]
pub fn load_visibility_thresholds(
    workspace_root: &Path,
    store_home: &Path,
) -> VisibilityThresholds {
    load_cordial_config(workspace_root, store_home).visibility
}

/// Convenience for the derives etiquette.
#[instrument(level = "info")]
pub fn load_derives_thresholds(workspace_root: &Path, store_home: &Path) -> DerivesThresholds {
    load_cordial_config(workspace_root, store_home).derives
}

#[instrument(level = "debug", skip(builder, path))]
fn add_optional_toml(
    builder: config::ConfigBuilder<config::builder::DefaultState>,
    path: &Path,
) -> config::ConfigBuilder<config::builder::DefaultState> {
    builder.add_source(File::from(path).format(FileFormat::Toml).required(false))
}
