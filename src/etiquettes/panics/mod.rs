mod assessor;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::PanicAssessor;
pub use enricher::PanicInventoryEnricher;
pub use probe::PanicSiteProbe;
pub use reporter::{PanicChecklistReporter, PanicCsvReporter, PanicSummaryReporter};
pub use scan::{scan_crate_panics, scan_rust_source, scan_source_tree};
pub use types::PanicKind;

use crate::etiquette::StaticEtiquette;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static PANIC_INVENTORY: PanicInventoryEnricher = PanicInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static PANIC_PROBE: PanicSiteProbe = PanicSiteProbe;
static PANIC_ASSESSOR: PanicAssessor = PanicAssessor;
static PANIC_CSV: PanicCsvReporter = PanicCsvReporter;
static PANIC_CHECKLIST: PanicChecklistReporter = PanicChecklistReporter;
static PANIC_SUMMARY: PanicSummaryReporter = PanicSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] =
    &[&SCOPE_ENRICHER, &PANIC_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&'static dyn crate::Probe] = &[&PANIC_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&PANIC_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[&PANIC_CSV, &PANIC_CHECKLIST, &PANIC_SUMMARY];

/// Built-in panics etiquette bundle.
pub static PANICS_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "panics",
    name: "Panic sources",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: None,
    reporters: REPORTERS,
    is_coverage: false,
};
