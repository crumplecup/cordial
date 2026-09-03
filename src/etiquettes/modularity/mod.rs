//! File, function, packing, and module-hierarchy size.
//!
//! **What.** Seven rules: oversized files and function bodies, too many types
//! per file, modules far from the crate mean (σ), top-heavy parents, lopsided
//! siblings, and unary child directories. See [`ModularityKind`].
//!
//! **Why.** Size and packing problems are split/extract signals. Visibility
//! asks whether a `pub mod` path has earned its existence; this etiquette
//! asks whether the *mass* in those modules should be peeled, split, or
//! collapsed. Companion to `visibility` and `cfg_scatter`.
//!
//! **How to use.** Run `cordial quality` (feature `modularity`). Thresholds
//! live under `[modularity]` in `cordial.toml`. Artifacts:
//! `{store}/findings/modularity.checklist.md`, `modularity-summary.md`, and
//! CSV. Register [`MODULARITY_ETIQUETTE`] on a [`crate::Session`].
//!
//! Policy: `docs/planning/modularity-etiquette.md`.

mod assessor;
mod enricher;
mod hierarchy;
mod probe;
mod quality_area;
mod reporter;
mod scan;
mod types;

pub use assessor::ModularityAssessor;
pub use enricher::ModularityInventoryEnricher;
pub use hierarchy::{
    ModuleHierarchyNode, ModuleSizeInput, OrderBand, SiblingImbalance, UnaryNest,
    build_module_hierarchy, fat_leaves, library_branches, lopsided_siblings, order_bands,
    top_heavy_parents, unary_nests,
};
pub use probe::ModularitySiteProbe;
pub use reporter::{ModularityChecklistReporter, ModularityCsvReporter, ModularitySummaryReporter};
pub use scan::scan_rust_source;
pub use types::{ModularityKind, ModuleSizeStats};

use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
    StaticQualityEtiquette,
};
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static MODULARITY_INVENTORY: ModularityInventoryEnricher = ModularityInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static MODULARITY_PROBE: ModularitySiteProbe = ModularitySiteProbe;
static MODULARITY_ASSESSOR: ModularityAssessor = ModularityAssessor;
static MODULARITY_CSV: ModularityCsvReporter = ModularityCsvReporter;
static MODULARITY_CHECKLIST: ModularityChecklistReporter = ModularityChecklistReporter;
static MODULARITY_SUMMARY: ModularitySummaryReporter = ModularitySummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] =
    &[&SCOPE_ENRICHER, &MODULARITY_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&'static dyn crate::Probe] = &[&MODULARITY_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&MODULARITY_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] =
    &[&MODULARITY_CSV, &MODULARITY_CHECKLIST, &MODULARITY_SUMMARY];

/// Built-in modularity etiquette bundle.
pub static MODULARITY_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette::new(
    StaticEtiquette::new(
        "modularity",
        "Modularity",
        EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
        false,
        EtiquetteExplain::new(
            "Which files, functions, and modules are too large or badly packed?",
            "Size and packing problems are split/extract signals. Visibility asks whether a pub mod path has earned its existence; this etiquette asks whether the mass in those modules should be peeled, split, or collapsed.",
            "Seven rules: oversized files and function bodies, too many types per file, modules far from the crate mean (σ), top-heavy parents, lopsided siblings, and unary child directories. Thresholds live under [modularity] in cordial.toml.",
            "`[modularity] enabled = false` in cordial.toml.",
            &[
                EtiquetteRuleExplain::new("MODULARITY-FILE", "File over the line threshold"),
                EtiquetteRuleExplain::new(
                    "MODULARITY-FUNCTION",
                    "Function body over the line threshold",
                ),
                EtiquetteRuleExplain::new(
                    "MODULARITY-TYPES-PER-FILE",
                    "Too many types in one file",
                ),
                EtiquetteRuleExplain::new(
                    "MODULARITY-MODULE-SIZE",
                    "Module size far from the crate mean",
                ),
                EtiquetteRuleExplain::new("MODULARITY-TOP-HEAVY", "Parent holds most of the mass"),
                EtiquetteRuleExplain::new(
                    "MODULARITY-LOPSIDED",
                    "Sibling modules are badly unbalanced",
                ),
                EtiquetteRuleExplain::new(
                    "MODULARITY-COLLAPSE",
                    "Unary child directory that should collapse",
                ),
            ],
        ),
    ),
    Some(QualityAreaSpec::new(
        "Modularity",
        "modularity.checklist.md",
        "modularity-summary.md",
        quality_area::quality_area_compute,
    )),
);
