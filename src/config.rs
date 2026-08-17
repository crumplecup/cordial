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

/// All etiquette knobs loaded from `cordial.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CordialConfig {
    #[serde(default)]
    pub visibility: VisibilityThresholds,
    #[serde(default)]
    pub modularity: ModularityThresholds,
    #[serde(default)]
    pub cfg_scatter: CfgScatterThresholds,
}

impl Default for CordialConfig {
    fn default() -> Self {
        Self {
            visibility: VisibilityThresholds::default(),
            modularity: ModularityThresholds::default(),
            cfg_scatter: CfgScatterThresholds::default(),
        }
    }
}

/// Visibility etiquette knobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisibilityThresholds {
    /// If the crate has fewer than this many externally reachable `pub` names,
    /// no `pub mod` is allowed on a public path.
    #[serde(default = "default_max_crate_names_for_flat")]
    pub max_crate_names_for_flat: usize,
    /// A visible module must contain at least this many leaf names.
    #[serde(default = "default_min_module_names")]
    pub min_module_names: usize,
    /// Prefer a fat root over modules smaller than [`Self::min_module_names`].
    #[serde(default = "default_prefer_root")]
    pub prefer_root: bool,
}

fn default_max_crate_names_for_flat() -> usize {
    50
}

fn default_min_module_names() -> usize {
    10
}

fn default_prefer_root() -> bool {
    true
}

impl VisibilityThresholds {
    pub fn new(max_crate_names_for_flat: usize, min_module_names: usize) -> Self {
        Self {
            max_crate_names_for_flat,
            min_module_names,
            prefer_root: default_prefer_root(),
        }
    }

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModularityThresholds {
    #[serde(default = "default_file_inventory_min_lines")]
    pub file_inventory_min_lines: u32,
    /// Track function and method bodies at least this long (CSV inventory).
    #[serde(default = "default_function_inventory_min_lines")]
    pub function_inventory_min_lines: u32,
    /// On files already at the file-inventory floor, name bodies at least this
    /// long as extract-helpers on the hotspot. Does not lower CSV inventory.
    #[serde(default = "default_function_hotspot_min_lines")]
    pub function_hotspot_min_lines: u32,
    #[serde(default = "default_file_checklist_min_lines")]
    pub file_checklist_min_lines: u32,
    /// Flag a function or method body this long as "split this body".
    #[serde(default = "default_function_checklist_min_lines")]
    pub function_checklist_min_lines: u32,
    /// Warn when a file defines more `struct`/`enum`/`union`/`trait` items than this.
    #[serde(default = "default_max_types_per_file")]
    pub max_types_per_file: u32,
    /// Flag a module when its size is more than this many sample standard
    /// deviations from the crate's mean module size.
    #[serde(default = "default_module_size_sigma")]
    pub module_size_sigma: u32,
    /// Exclude modules smaller than this from the 2σ sample. `0` includes all.
    #[serde(default = "default_min_module_lines")]
    pub min_module_lines: u32,
    /// Checklist a parent that kept at least this percent of its subtree
    /// (`own * 100 / subtree`).
    #[serde(default = "default_top_heavy_min_percent")]
    pub top_heavy_min_percent: u32,
    /// Checklist when one child holds at least this percent of its siblings'
    /// combined subtree (siblings below `hierarchy_min_lines` are ignored).
    #[serde(default = "default_lopsided_min_percent")]
    pub lopsided_min_percent: u32,
    /// Ignore hierarchy hits whose parent own-lines (top-heavy) or dominant
    /// subtree (lopsided) is smaller than this.
    #[serde(default = "default_hierarchy_min_lines")]
    pub hierarchy_min_lines: u32,
}

fn default_file_inventory_min_lines() -> u32 {
    500
}

fn default_function_inventory_min_lines() -> u32 {
    150
}

fn default_function_hotspot_min_lines() -> u32 {
    80
}

fn default_file_checklist_min_lines() -> u32 {
    1000
}

fn default_function_checklist_min_lines() -> u32 {
    200
}

fn default_max_types_per_file() -> u32 {
    10
}

fn default_module_size_sigma() -> u32 {
    2
}

fn default_min_module_lines() -> u32 {
    0
}

fn default_top_heavy_min_percent() -> u32 {
    50
}

fn default_lopsided_min_percent() -> u32 {
    75
}

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
            min_module_lines: default_min_module_lines(),
            top_heavy_min_percent: default_top_heavy_min_percent(),
            lopsided_min_percent: default_lopsided_min_percent(),
            hierarchy_min_lines: default_hierarchy_min_lines(),
        }
    }
}

impl ModularityThresholds {
    pub fn ratio_meets(numerator: u32, denominator: u32, percent: u32) -> bool {
        denominator > 0 && u64::from(numerator) * 100 >= u64::from(denominator) * u64::from(percent)
    }

    pub fn is_top_heavy_hit(&self, own_lines: u32, subtree_lines: u32) -> bool {
        own_lines >= self.hierarchy_min_lines
            && Self::ratio_meets(own_lines, subtree_lines, self.top_heavy_min_percent)
    }

    pub fn is_lopsided_hit(&self, largest_subtree: u32, sibling_total: u32) -> bool {
        largest_subtree >= self.hierarchy_min_lines
            && Self::ratio_meets(largest_subtree, sibling_total, self.lopsided_min_percent)
    }

    /// Function-body floor used while scanning one file.
    ///
    /// Inventory-sized files also record shorter bodies so too-long hotspots
    /// can name extract-helper candidates. CSV inventory stays at
    /// [`Self::function_inventory_min_lines`].
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CfgScatterThresholds {
    #[serde(default = "default_min_distinct_kinds")]
    pub min_distinct_kinds: usize,
    #[serde(default = "default_min_occurrences")]
    pub min_occurrences: usize,
}

fn default_min_distinct_kinds() -> usize {
    2
}

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

/// Load `cordial.toml` from the workspace and `{store_home}/cordial.toml`,
/// layered over [`CordialConfig::default`]. Workspace wins. Missing or
/// unreadable files fall back to `Default` instead of failing the run.
#[tracing::instrument]
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
pub fn load_session_config(session: &dyn SessionView) -> CordialConfig {
    load_cordial_config(session.project_root(), session.store_home())
}

/// Convenience for the visibility etiquette.
#[tracing::instrument]
pub fn load_visibility_thresholds(
    workspace_root: &Path,
    store_home: &Path,
) -> VisibilityThresholds {
    load_cordial_config(workspace_root, store_home).visibility
}

fn add_optional_toml(
    builder: config::ConfigBuilder<config::builder::DefaultState>,
    path: &Path,
) -> config::ConfigBuilder<config::builder::DefaultState> {
    builder.add_source(File::from(path).format(FileFormat::Toml).required(false))
}
