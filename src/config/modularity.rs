use serde::{Deserialize, Serialize};
use tracing::instrument;

use super::default_true;

/// Modularity etiquette knobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct ModularityThresholds {
    /// File inventory floor. Files at or above this size are tracked in
    /// inventory, and the default checklist floor matches this reader budget.
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
    /// When true, hide lower-tail (`z < -sigma`) diagnostics in module-size
    /// summary context. Module z-scores do not become action items.
    #[serde(default)]
    #[getter(copy)]
    module_size_ignore_lower_tail: bool,
    /// Exclude modules smaller than this from the 2sigma sample. `0` includes all.
    /// This is a sample filter, not a checklist floor.
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
    default_file_inventory_min_lines()
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
    #[instrument(level = "debug")]
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
    #[instrument(level = "trace", skip(self))]
    pub fn with_module_size_ignore_lower_tail(self, ignore: bool) -> Self {
        Self {
            module_size_ignore_lower_tail: ignore,
            ..self
        }
    }

    /// Return a copy with `file_inventory_min_lines` set.
    #[instrument(level = "trace", skip(self))]
    pub fn with_file_inventory_min_lines(self, value: u32) -> Self {
        Self {
            file_inventory_min_lines: value,
            ..self
        }
    }

    /// Return a copy with `function_inventory_min_lines` set.
    #[instrument(level = "trace", skip(self))]
    pub fn with_function_inventory_min_lines(self, value: u32) -> Self {
        Self {
            function_inventory_min_lines: value,
            ..self
        }
    }

    /// Return a copy with `function_hotspot_min_lines` set.
    #[instrument(level = "trace", skip(self))]
    pub fn with_function_hotspot_min_lines(self, value: u32) -> Self {
        Self {
            function_hotspot_min_lines: value,
            ..self
        }
    }

    /// Return a copy with `file_checklist_min_lines` set.
    #[instrument(level = "trace", skip(self))]
    pub fn with_file_checklist_min_lines(self, value: u32) -> Self {
        Self {
            file_checklist_min_lines: value,
            ..self
        }
    }

    /// Return a copy with `function_checklist_min_lines` set.
    #[instrument(level = "trace", skip(self))]
    pub fn with_function_checklist_min_lines(self, value: u32) -> Self {
        Self {
            function_checklist_min_lines: value,
            ..self
        }
    }

    /// Return a copy with `max_types_per_file` set.
    #[instrument(level = "trace", skip(self))]
    pub fn with_max_types_per_file(self, value: u32) -> Self {
        Self {
            max_types_per_file: value,
            ..self
        }
    }

    /// Return a copy with `lopsided_min_percent` set.
    #[instrument(level = "trace", skip(self))]
    pub fn with_lopsided_min_percent(self, value: u32) -> Self {
        Self {
            lopsided_min_percent: value,
            ..self
        }
    }

    /// Return a copy with `hierarchy_min_lines` set.
    #[instrument(level = "trace", skip(self))]
    pub fn with_hierarchy_min_lines(self, value: u32) -> Self {
        Self {
            hierarchy_min_lines: value,
            ..self
        }
    }

    /// Return a copy with `generated_files` set.
    #[instrument(level = "trace", skip(self))]
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

    /// MODULE-SIZE rows are diagnostic context, not checklist items.
    ///
    /// The actionable file-size queue is driven by
    /// [`Self::file_checklist_min_lines`]. Z-scores stay in the CSV and
    /// summary so reviewers can see unusual module shapes without turning
    /// a moving statistical threshold into a moving action list.
    #[instrument(level = "trace", skip(self))]
    pub fn is_module_size_checklist(&self) -> bool {
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
