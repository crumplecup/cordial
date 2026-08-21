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
    tracing: TracingThresholds,
    #[serde(default)]
    derives: DerivesThresholds,
}

/// Visibility etiquette knobs.
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
        }
    }
}

/// Modularity etiquette knobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
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
        }
    }
}

impl ModularityThresholds {
    #[instrument(level = "debug")]
    pub fn with_module_size_ignore_lower_tail(self, ignore: bool) -> Self {
        Self {
            module_size_ignore_lower_tail: ignore,
            ..self
        }
    }

    #[instrument(level = "debug")]
    pub fn with_file_inventory_min_lines(self, value: u32) -> Self {
        Self {
            file_inventory_min_lines: value,
            ..self
        }
    }

    #[instrument(level = "debug")]
    pub fn with_function_inventory_min_lines(self, value: u32) -> Self {
        Self {
            function_inventory_min_lines: value,
            ..self
        }
    }

    #[instrument(level = "debug")]
    pub fn with_function_hotspot_min_lines(self, value: u32) -> Self {
        Self {
            function_hotspot_min_lines: value,
            ..self
        }
    }

    #[instrument(level = "debug")]
    pub fn with_file_checklist_min_lines(self, value: u32) -> Self {
        Self {
            file_checklist_min_lines: value,
            ..self
        }
    }

    #[instrument(level = "debug")]
    pub fn with_function_checklist_min_lines(self, value: u32) -> Self {
        Self {
            function_checklist_min_lines: value,
            ..self
        }
    }

    #[instrument(level = "debug")]
    pub fn with_max_types_per_file(self, value: u32) -> Self {
        Self {
            max_types_per_file: value,
            ..self
        }
    }

    #[instrument(level = "debug")]
    pub fn with_lopsided_min_percent(self, value: u32) -> Self {
        Self {
            lopsided_min_percent: value,
            ..self
        }
    }

    #[instrument(level = "debug")]
    pub fn with_hierarchy_min_lines(self, value: u32) -> Self {
        Self {
            hierarchy_min_lines: value,
            ..self
        }
    }

    #[instrument(level = "debug")]
    pub fn ratio_meets(numerator: u32, denominator: u32, percent: u32) -> bool {
        denominator > 0 && u64::from(numerator) * 100 >= u64::from(denominator) * u64::from(percent)
    }

    #[instrument(level = "trace", skip(self))]
    pub fn is_top_heavy_hit(&self, own_lines: u32, subtree_lines: u32) -> bool {
        own_lines >= self.hierarchy_min_lines
            && Self::ratio_meets(own_lines, subtree_lines, self.top_heavy_min_percent)
    }

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
}

impl Default for TracingThresholds {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            extra_skip: Vec::new(),
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
