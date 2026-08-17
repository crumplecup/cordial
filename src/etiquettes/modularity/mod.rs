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
    ModuleHierarchyNode, ModuleSizeInput, OrderBand, SiblingImbalance, build_module_hierarchy,
    fat_leaves, library_branches, lopsided_siblings, order_bands, top_heavy_parents,
};
pub use probe::ModularitySiteProbe;
pub use reporter::{ModularityChecklistReporter, ModularityCsvReporter, ModularitySummaryReporter};
pub use scan::scan_rust_source;
pub use types::{ModularityKind, ModularityThresholds, ModuleSizeStats};

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
