//! Internal error types as a graph, plus compliance.
//!
//! **What.** Builds the crate’s error-type graph and enforces a rigid error
//! architecture. The catalog is every type that implements `Error` (or
//! `#[derive(Error)]`) under `src/`: a parent error boxes an umbrella `*Kind`
//! enum; every Kind variant holds a native source; native sources that wrap a
//! foreign error keep it in `source` with owned `file`/`line` copied from
//! `Location::caller()` and `#[track_caller]`;
//! nested native sources may box another Kind and the same rules recurse.
//! Native sources may live next to their call site.
//!
//! **Why.** Foreign-error etiquettes ask what leaks *in*. This one asks
//! whether *our* error types are a place those leaks can land. Without a
//! typed internal graph, attenuation advice has nowhere to point.
//!
//! **How to use.** Run `cordial quality` (feature `internal_error_chain`).
//! Artifacts: `{store}/findings/internal-error-chain.checklist.md`,
//! `internal-error-chain-summary.md`, type-graph and compliance CSVs.
//! Register [`INTERNAL_ERROR_CHAIN_ETIQUETTE`].
//!
//! Policy: `docs/planning/error-handling-as-plugin.md`.

mod architecture;
mod assessor;
mod compliance;
mod probe;
mod reporter;
mod scan;
mod source_shape;
mod type_graph;
mod types;

pub(crate) use architecture::scan_crate_error_architecture;
pub use assessor::InternalErrorChainAssessor;
pub use probe::InternalErrorChainProbe;
pub use reporter::{
    InternalErrorChainChecklistReporter, InternalErrorChainSummaryReporter,
    InternalErrorComplianceCsvReporter, InternalErrorTypeGraphCsvReporter,
};
pub use scan::{
    scan_compliance_rust_source, scan_crate_internal_error_chain, scan_error_rust_source,
};
pub(crate) use type_graph::{
    RawTypeNode, finalize_type_graph, scan_error_rust_syntax_raw, type_path_is_error_related,
};
pub use types::{
    InternalErrorChainScanReport, InternalErrorComplianceFinding, InternalErrorComplianceId,
    InternalErrorComplianceReport, InternalErrorNodeClass, InternalErrorTypeGraphReport,
    InternalErrorTypeNode, InternalErrorTypeProbeId, WorkspaceInternalErrorChainSummary,
    build_workspace_internal_error_chain_summary,
};

use crate::SourceLoader;
use crate::enricher::ERROR_IR_ENRICHERS;
use crate::etiquette::{
    EtiquetteExplain, EtiquetteRuleExplain, StaticEtiquette, StaticQualityEtiquette,
};

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
pub static INTERNAL_ERROR_CHAIN_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette {
    etiquette: StaticEtiquette {
        id: "internal_error_chain",
        name: "Internal error chain",
        loaders: LOADERS,
        enrichers: ENRICHERS,
        probes: PROBES,
        assessors: ASSESSORS,
        workspace_assessors: None,
        reporters: REPORTERS,
        is_coverage: false,
        explain: EtiquetteExplain {
            summary: "Do this crate's error types form the parent / Kind / native-source architecture?",
            why: "Foreign-error etiquettes ask what leaks in. This one asks whether our error types are a place those leaks can land.",
            logic: "Catalogs every type that implements Error under src/. A parent boxes an umbrella *Kind enum; every Kind variant holds a native source; native sources that wrap a foreign error keep it in source with owned file/line from Location::caller() and #[track_caller]. Nested native sources may box another Kind. Feeds the hand-composed Error handling quality-report area.",
            opt_out: "`[internal_error_chain] enabled = false` in cordial.toml.",
            rules: &[
                EtiquetteRuleExplain {
                    id: "ERROR-CHAIN-COMPLIANCE-STRINGIFY-001",
                    summary: "Stringifies a foreign error",
                },
                EtiquetteRuleExplain {
                    id: "ERROR-CHAIN-COMPLIANCE-DISCARD-TYPED-001",
                    summary: "Discards a typed error",
                },
                EtiquetteRuleExplain {
                    id: "ERROR-CHAIN-COMPLIANCE-SOURCE-SHAPE-001",
                    summary: "Native source shape is wrong",
                },
                EtiquetteRuleExplain {
                    id: "ERROR-CHAIN-COMPLIANCE-SOURCE-TRACK-CALLER-001",
                    summary: "Native source missing #[track_caller]",
                },
                EtiquetteRuleExplain {
                    id: "ERROR-CHAIN-COMPLIANCE-ARCH-PARENT-001",
                    summary: "Parent does not box Kind",
                },
                EtiquetteRuleExplain {
                    id: "ERROR-CHAIN-COMPLIANCE-ARCH-KIND-BOX-001",
                    summary: "Kind is not boxed",
                },
                EtiquetteRuleExplain {
                    id: "ERROR-CHAIN-COMPLIANCE-ARCH-KIND-VARIANT-001",
                    summary: "Kind variant is not a native source",
                },
                EtiquetteRuleExplain {
                    id: "ERROR-CHAIN-COMPLIANCE-ARCH-ORPHAN-SOURCE-001",
                    summary: "Orphan native source",
                },
                EtiquetteRuleExplain {
                    id: "ERROR-CHAIN-INTERNAL-LEAF-001",
                    summary: "Internal leaf in the type graph",
                },
                EtiquetteRuleExplain {
                    id: "ERROR-CHAIN-INTERNAL-LINK-001",
                    summary: "Internal link in the type graph",
                },
                EtiquetteRuleExplain {
                    id: "ERROR-CHAIN-INTERNAL-NESTED-001",
                    summary: "Nested internal error",
                },
            ],
        },
    },
    // Declines a dedicated row on purpose: its COMPLIANCE violation
    // count feeds the hand-composed "Error handling" area instead (see
    // reporter::quality_report).
    quality_area: None,
};
