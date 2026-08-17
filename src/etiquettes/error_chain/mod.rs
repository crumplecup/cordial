mod assessor;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::ErrorChainAssessor;
pub use probe::ErrorChainProbe;
pub use reporter::{ErrorChainChecklistReporter, ErrorChainCsvReporter, ErrorChainSummaryReporter};
pub use scan::{scan_crate_error_chain, scan_rust_source};
pub use types::{ErrorChainProbeId, ErrorChainRecord, probe_counts};

use crate::SourceLoader;
use crate::enricher::ERROR_IR_ENRICHERS;
use crate::etiquette::StaticEtiquette;

static SOURCE_LOADER: SourceLoader = SourceLoader;
static ERROR_CHAIN_PROBE: ErrorChainProbe = ErrorChainProbe;
static ERROR_CHAIN_ASSESSOR: ErrorChainAssessor = ErrorChainAssessor;
static ERROR_CHAIN_CSV: ErrorChainCsvReporter = ErrorChainCsvReporter;
static ERROR_CHAIN_CHECKLIST: ErrorChainChecklistReporter = ErrorChainChecklistReporter;
static ERROR_CHAIN_SUMMARY: ErrorChainSummaryReporter = ErrorChainSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] = ERROR_IR_ENRICHERS;
static PROBES: &[&'static dyn crate::Probe] = &[&ERROR_CHAIN_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&ERROR_CHAIN_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &ERROR_CHAIN_CSV,
    &ERROR_CHAIN_CHECKLIST,
    &ERROR_CHAIN_SUMMARY,
];

/// Built-in error chain preservation etiquette bundle.
pub static ERROR_CHAIN_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "error_chain",
    name: "Error chain preservation",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: None,
    reporters: REPORTERS,
    is_coverage: false,
};
