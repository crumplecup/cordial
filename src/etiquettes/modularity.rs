//! Code organization and readability, guided by size and hierarchy signals.
//!
//! **What.** Finds files, functions, and module hierarchies whose organization
//! needs examination.
//!
//! **Why.** A human reader should be able to locate a responsibility, understand
//! it with limited context, and follow its dependencies. Extractions should group
//! coherent responsibilities behind meaningful names and preserve useful locality.
//!
//! **Flags.** Oversized files and function bodies, too many types per file,
//! modules far from the crate mean, top-heavy parents, lopsided siblings, and
//! unary child directories.
//!
//! **Ignores.** Public-path justification belongs to `visibility`.
//! Conditional-code placement belongs to `cfg_scatter`.
//!
//! **Outputs.** `{store}/findings/modularity.checklist.md`,
//! `modularity-summary.md`, and CSV.
//!
//! **Config.** `[modularity]` owns the thresholds and `enabled`. Register
//! [`MODULARITY_ETIQUETTE`] on a [`crate::Session`]. Policy:
//! `docs/planning/modularity-etiquette.md`.

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

use std::sync::LazyLock;

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
pub static MODULARITY_ETIQUETTE: LazyLock<StaticQualityEtiquette> = LazyLock::new(|| {
    StaticQualityEtiquette::new(
        StaticEtiquette::new(
            "modularity",
            "Modularity",
            EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
            false,
            EtiquetteExplain::new(
                "Is the code organized so a human reader can find and understand each responsibility?",
                "Organize code so a human reader can locate a responsibility, understand it with limited context, and follow its dependencies.",
                "Use size and hierarchy findings to identify places that need examination. Group closely related behavior behind meaningful module names, keep collaborating code together, and give each extraction a clear purpose. Before editing, consider the organization of the affected module as a whole; after editing, review whether the resulting structure makes the code easier to navigate and understand. Seven rules provide signals: oversized files and function bodies, too many types per file, modules far from the crate mean (σ), top-heavy parents, lopsided siblings, and unary child directories. Thresholds live under [modularity] in cordial.toml.",
                "`[modularity] enabled = false` in cordial.toml.",
                vec![
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
                    EtiquetteRuleExplain::new(
                        "MODULARITY-TOP-HEAVY",
                        "Parent holds most of the mass",
                    ),
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
    )
});
