mod assessor;
mod foreign_types;
mod probe;
mod reporter;
mod types;

pub use assessor::ForeignErrorTypeAssessor;
pub use foreign_types::{build_error_site_partition_report, build_foreign_error_type_report};
pub use probe::ForeignErrorTypeProbe;
pub use reporter::{
    ForeignErrorTypesChecklistReporter, ForeignErrorTypesCsvReporter,
    ForeignErrorTypesSummaryReporter, ForeignErrorsChecklistReporter,
};
pub use types::{
    ErrorSitePartitionReport, ForeignErrorRecordKind, ForeignErrorTypeRecord,
    ForeignErrorTypeReport, WorkspaceForeignErrorTypeSummary,
    build_workspace_foreign_error_type_summary,
};

use crate::SourceLoader;
use crate::enricher::ERROR_IR_ENRICHERS;
use crate::etiquette::StaticEtiquette;

static SOURCE_LOADER: SourceLoader = SourceLoader;
static FOREIGN_ERROR_TYPE_PROBE: ForeignErrorTypeProbe = ForeignErrorTypeProbe;
static FOREIGN_ERROR_TYPE_ASSESSOR: ForeignErrorTypeAssessor = ForeignErrorTypeAssessor;
static FOREIGN_ERROR_TYPES_CSV: ForeignErrorTypesCsvReporter = ForeignErrorTypesCsvReporter;
static FOREIGN_ERROR_TYPES_CHECKLIST: ForeignErrorTypesChecklistReporter =
    ForeignErrorTypesChecklistReporter;
static FOREIGN_ERROR_TYPES_SUMMARY: ForeignErrorTypesSummaryReporter =
    ForeignErrorTypesSummaryReporter;
static FOREIGN_ERRORS_CHECKLIST: ForeignErrorsChecklistReporter = ForeignErrorsChecklistReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] = ERROR_IR_ENRICHERS;
static PROBES: &[&'static dyn crate::Probe] = &[&FOREIGN_ERROR_TYPE_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&FOREIGN_ERROR_TYPE_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &FOREIGN_ERROR_TYPES_CSV,
    &FOREIGN_ERROR_TYPES_CHECKLIST,
    &FOREIGN_ERROR_TYPES_SUMMARY,
    &FOREIGN_ERRORS_CHECKLIST,
];

/// Built-in foreign error types etiquette bundle.
pub static FOREIGN_ERROR_TYPES_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "foreign_error_types",
    name: "Foreign error types",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: None,
    reporters: REPORTERS,
    is_coverage: false,
};
