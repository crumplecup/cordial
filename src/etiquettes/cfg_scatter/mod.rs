mod assessor;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::CfgScatterAssessor;
pub use enricher::CfgScatterInventoryEnricher;
pub use probe::CfgScatterSiteProbe;
pub use reporter::{CfgScatterChecklistReporter, CfgScatterCsvReporter, CfgScatterSummaryReporter};
pub use scan::scan_rust_source;
pub use types::{CfgScatterThresholds, CfgSiteKind};

use crate::etiquette::StaticEtiquette;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static CFG_SCATTER_INVENTORY: CfgScatterInventoryEnricher = CfgScatterInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static CFG_SCATTER_PROBE: CfgScatterSiteProbe = CfgScatterSiteProbe;
static CFG_SCATTER_ASSESSOR: CfgScatterAssessor = CfgScatterAssessor;
static CFG_SCATTER_CSV: CfgScatterCsvReporter = CfgScatterCsvReporter;
static CFG_SCATTER_CHECKLIST: CfgScatterChecklistReporter = CfgScatterChecklistReporter;
static CFG_SCATTER_SUMMARY: CfgScatterSummaryReporter = CfgScatterSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] =
    &[&SCOPE_ENRICHER, &CFG_SCATTER_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&'static dyn crate::Probe] = &[&CFG_SCATTER_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&CFG_SCATTER_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &CFG_SCATTER_CSV,
    &CFG_SCATTER_CHECKLIST,
    &CFG_SCATTER_SUMMARY,
];

/// Built-in cfg-scatter etiquette bundle: flags a `#[cfg(...)]` predicate
/// repeated across multiple item kinds in one file (functions, impls,
/// imports, …) that would be clearer as a single `#[cfg]`-gated `mod`.
/// Field/variant-only gating is never flagged — see [`CfgSiteKind`] docs.
pub static CFG_SCATTER_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "cfg_scatter",
    name: "Scattered cfg predicates",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: None,
    reporters: REPORTERS,
    is_coverage: false,
};
