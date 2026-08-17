mod apply;
mod assessor;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod types;

pub use apply::{
    InstrumentApplySummary, InstrumentGap, parse_tracing_instrument_checklist,
    parse_tracing_instrument_checklist_text, run_tracing_instrument_apply,
};

pub use assessor::TracingAssessor;
pub use enricher::FunctionInventoryEnricher;
pub use probe::MissingInstrumentProbe;
pub use reporter::{TracingChecklistReporter, TracingCsvReporter, TracingSummaryReporter};
pub use scan::scan_rust_source;

use crate::etiquette::StaticEtiquette;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static FUNCTION_INVENTORY: FunctionInventoryEnricher = FunctionInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static MISSING_INSTRUMENT_PROBE: MissingInstrumentProbe = MissingInstrumentProbe;
static TRACING_ASSESSOR: TracingAssessor = TracingAssessor;
static TRACING_CSV: TracingCsvReporter = TracingCsvReporter;
static TRACING_CHECKLIST: TracingChecklistReporter = TracingChecklistReporter;
static TRACING_SUMMARY: TracingSummaryReporter = TracingSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] =
    &[&SCOPE_ENRICHER, &FUNCTION_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&'static dyn crate::Probe] = &[&MISSING_INSTRUMENT_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&TRACING_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] =
    &[&TRACING_CSV, &TRACING_CHECKLIST, &TRACING_SUMMARY];

/// Built-in tracing instrument etiquette bundle.
pub static TRACING_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "tracing",
    name: "Tracing instrument",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: None,
    reporters: REPORTERS,
    is_coverage: false,
};
