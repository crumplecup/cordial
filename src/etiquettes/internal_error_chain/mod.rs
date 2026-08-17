mod assessor;
mod compliance;
mod probe;
mod reporter;
mod scan;
mod type_graph;
mod types;

pub use assessor::InternalErrorChainAssessor;
pub use probe::InternalErrorChainProbe;
pub use reporter::{
    InternalErrorChainChecklistReporter, InternalErrorChainSummaryReporter,
    InternalErrorComplianceCsvReporter, InternalErrorTypeGraphCsvReporter,
};
pub use scan::{
    scan_compliance_rust_source, scan_crate_internal_error_chain, scan_error_rust_source,
};
pub(crate) use type_graph::{RawTypeNode, finalize_type_graph, scan_error_rust_syntax_raw};
pub use types::{
    InternalErrorChainScanReport, InternalErrorComplianceFinding, InternalErrorComplianceId,
    InternalErrorComplianceReport, InternalErrorNodeClass, InternalErrorTypeGraphReport,
    InternalErrorTypeNode, InternalErrorTypeProbeId, WorkspaceInternalErrorChainSummary,
    build_workspace_internal_error_chain_summary,
};

use crate::SourceLoader;
use crate::enricher::ERROR_IR_ENRICHERS;
use crate::etiquette::StaticEtiquette;

static SOURCE_LOADER: SourceLoader = SourceLoader;
static INTERNAL_ERROR_CHAIN_PROBE: InternalErrorChainProbe = InternalErrorChainProbe;
static INTERNAL_ERROR_CHAIN_ASSESSOR: InternalErrorChainAssessor = InternalErrorChainAssessor;
static INTERNAL_ERROR_TYPE_GRAPH_CSV: InternalErrorTypeGraphCsvReporter =
    InternalErrorTypeGraphCsvReporter;
static INTERNAL_ERROR_COMPLIANCE_CSV: InternalErrorComplianceCsvReporter =
    InternalErrorComplianceCsvReporter;
static INTERNAL_ERROR_CHAIN_CHECKLIST: InternalErrorChainChecklistReporter =
    InternalErrorChainChecklistReporter;
static INTERNAL_ERROR_CHAIN_SUMMARY: InternalErrorChainSummaryReporter =
    InternalErrorChainSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] = ERROR_IR_ENRICHERS;
static PROBES: &[&'static dyn crate::Probe] = &[&INTERNAL_ERROR_CHAIN_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&INTERNAL_ERROR_CHAIN_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &INTERNAL_ERROR_TYPE_GRAPH_CSV,
    &INTERNAL_ERROR_COMPLIANCE_CSV,
    &INTERNAL_ERROR_CHAIN_CHECKLIST,
    &INTERNAL_ERROR_CHAIN_SUMMARY,
];

/// Built-in internal error chain etiquette bundle.
pub static INTERNAL_ERROR_CHAIN_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "internal_error_chain",
    name: "Internal error chain",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: None,
    reporters: REPORTERS,
    is_coverage: false,
};
