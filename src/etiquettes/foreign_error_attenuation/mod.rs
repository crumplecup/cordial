mod assess;
mod assessor;
mod enricher;
mod probe;
mod reporter;
mod types;

pub use assess::build_foreign_error_attenuation_report;
pub use assessor::ForeignErrorAttenuationAssessor;
pub use enricher::ForeignErrorAttenuationInventoryEnricher;
pub use probe::ForeignErrorAttenuationProbe;
pub use reporter::{
    ForeignErrorAttenuationChecklistReporter, ForeignErrorAttenuationCsvReporter,
    ForeignErrorAttenuationSummaryReporter,
};
pub use types::{
    ForeignErrorAttenuationReport, ForeignErrorHandlingClass,
    WorkspaceForeignErrorAttenuationSummary, build_workspace_foreign_error_attenuation_summary,
};

use crate::SourceLoader;
use crate::enricher::ERROR_IR_ENRICHERS;
use crate::etiquette::StaticEtiquette;

static SOURCE_LOADER: SourceLoader = SourceLoader;
static FOREIGN_ERROR_ATTENUATION_PROBE: ForeignErrorAttenuationProbe = ForeignErrorAttenuationProbe;
static FOREIGN_ERROR_ATTENUATION_ASSESSOR: ForeignErrorAttenuationAssessor =
    ForeignErrorAttenuationAssessor;
static FOREIGN_ERROR_ATTENUATION_CSV: ForeignErrorAttenuationCsvReporter =
    ForeignErrorAttenuationCsvReporter;
static FOREIGN_ERROR_ATTENUATION_CHECKLIST: ForeignErrorAttenuationChecklistReporter =
    ForeignErrorAttenuationChecklistReporter;
static FOREIGN_ERROR_ATTENUATION_SUMMARY: ForeignErrorAttenuationSummaryReporter =
    ForeignErrorAttenuationSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] = ERROR_IR_ENRICHERS;
static PROBES: &[&'static dyn crate::Probe] = &[&FOREIGN_ERROR_ATTENUATION_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&FOREIGN_ERROR_ATTENUATION_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &FOREIGN_ERROR_ATTENUATION_CSV,
    &FOREIGN_ERROR_ATTENUATION_CHECKLIST,
    &FOREIGN_ERROR_ATTENUATION_SUMMARY,
];

/// Built-in foreign error attenuation etiquette bundle.
pub static FOREIGN_ERROR_ATTENUATION_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "foreign_error_attenuation",
    name: "Foreign error attenuation",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: None,
    reporters: REPORTERS,
    is_coverage: false,
};
