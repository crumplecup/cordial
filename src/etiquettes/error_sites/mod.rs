mod assessor;
mod foreign_infer;
mod partition;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::ErrorSiteAssessor;
pub use foreign_infer::{ForeignTypeConfidence, infer_foreign_error_type};
pub use partition::{
    PartitionedErrorSiteRow, partition_error_site_records, partition_error_site_row,
};
pub use probe::ErrorSiteProbe;
pub use reporter::{
    ErrorSitesChecklistReporter, ErrorSitesCsvReporter, ErrorSitesPartitionSummaryReporter,
    ErrorSitesPartitionedCsvReporter, ErrorSitesSummaryReporter,
};
pub use scan::{scan_crate_error_sites, scan_rust_source};
pub use types::{
    ErrorOriginClass, ErrorSiteKind, ErrorSiteRecord, ErrorSiteScanRow, ForeignErrorRecordKind,
};

use crate::SourceLoader;
use crate::enricher::ERROR_IR_ENRICHERS;
use crate::etiquette::StaticEtiquette;

static SOURCE_LOADER: SourceLoader = SourceLoader;
static ERROR_SITE_PROBE: ErrorSiteProbe = ErrorSiteProbe;
static ERROR_SITE_ASSESSOR: ErrorSiteAssessor = ErrorSiteAssessor;
static ERROR_SITES_CSV: ErrorSitesCsvReporter = ErrorSitesCsvReporter;
static ERROR_SITES_CHECKLIST: ErrorSitesChecklistReporter = ErrorSitesChecklistReporter;
static ERROR_SITES_SUMMARY: ErrorSitesSummaryReporter = ErrorSitesSummaryReporter;
static ERROR_SITES_PARTITIONED_CSV: ErrorSitesPartitionedCsvReporter =
    ErrorSitesPartitionedCsvReporter;
static ERROR_SITES_PARTITION_SUMMARY: ErrorSitesPartitionSummaryReporter =
    ErrorSitesPartitionSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] = ERROR_IR_ENRICHERS;
static PROBES: &[&'static dyn crate::Probe] = &[&ERROR_SITE_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&ERROR_SITE_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &ERROR_SITES_CSV,
    &ERROR_SITES_CHECKLIST,
    &ERROR_SITES_SUMMARY,
    &ERROR_SITES_PARTITIONED_CSV,
    &ERROR_SITES_PARTITION_SUMMARY,
];

/// Built-in error sites etiquette bundle.
pub static ERROR_SITES_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "error_sites",
    name: "Error sites",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: None,
    reporters: REPORTERS,
    is_coverage: false,
};
