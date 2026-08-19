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

use crate::etiquette::StaticEtiquette;
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
pub static MODULARITY_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "modularity",
    name: "Modularity",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: None,
    reporters: REPORTERS,
    is_coverage: false,
};
