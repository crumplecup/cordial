//! Internal error types as a graph, plus compliance.
//!
//! **What.** Builds the crate's error-type graph and enforces the
//! parent / `Kind` / native-source architecture.
//!
//! **Why.** Foreign-error etiquettes ask what leaks in. This etiquette asks
//! whether the crate-owned error types are a place those leaks can land.
//! Without a typed internal graph, attenuation advice has nowhere to point.
//!
//! **Flags.** Parent errors that do not hold an umbrella `*Kind` as configured, kind enums
//! with wrong payload shapes, orphan native sources, stringified or discarded
//! typed errors, Kind variants with diverse literal fan-out, native sources
//! missing `#[track_caller]`, function-item
//! conversions that lose the caller location, and internal leaf /
//! link / nested graph shapes. Native sources that wrap a foreign error keep
//! it in `source` with `file` / `line` copied from `Location::caller()`.
//!
//! **Ignores.** Native sources may live next to their call site. Nested native
//! sources may hold another `Kind` when the same architecture recurses.
//!
//! **Outputs.** `{store}/findings/internal-error-chain.checklist.md`,
//! `internal-error-chain-summary.md`, type-graph CSVs, and compliance CSVs.
//!
//! **Config.** `[internal_error_chain] enabled = false` opts out;
//! `kind_fanout_max_call_sites = 0` is the default and flags every matching
//! literal call site.
//! `field_strategy = "borrowed"` requires borrowed non-Copy fields, including
//! `kind`, `source`, and `file`; the default is `"owned"`.
//! Set these in `cordial.toml`. Register [`INTERNAL_ERROR_CHAIN_ETIQUETTE`]. Policy:
//! `docs/planning/error-handling-as-plugin.md`.

mod apply;
mod architecture;
mod assessor;
mod compliance;
mod probe;
mod reporter;
mod scan;
mod signature_chain;
mod source_shape;
mod type_graph;
mod types;

pub(crate) use apply::enrich_conversion_units;
pub use apply::{
    ErrorConversionSite, ErrorConversionSiteKind, ErrorConversionUnit, ErrorEdgeEvidence,
    ErrorEdgeField, ErrorEdgeStorage, ErrorEdgeTarget, ErrorEdgeTargetKind, ErrorFieldApplyBlocker,
    ErrorFieldApplyBlockerReason, ErrorFieldApplySummary, ErrorFileApplySummary,
    run_error_field_apply, run_error_file_apply,
};
pub(crate) use architecture::scan_crate_error_architecture_with_source;
pub use assessor::InternalErrorChainAssessor;
pub use probe::InternalErrorChainProbe;
pub use reporter::{
    InternalErrorChainChecklistReporter, InternalErrorChainSummaryReporter,
    InternalErrorComplianceCsvReporter, InternalErrorTypeGraphCsvReporter,
};
pub use scan::{
    scan_compliance_rust_source, scan_crate_internal_error_chain, scan_error_rust_source,
};
pub(crate) use signature_chain::SignatureChainIndex;
pub use signature_chain::{SignatureChainCheck, SignatureChainStatus};
pub(crate) use type_graph::{
    RawTypeNode, finalize_type_graph, for_each_shared_src_rust_file, scan_error_rust_syntax_raw,
    type_path_is_error_related,
};
pub use types::{
    InternalErrorChainScanReport, InternalErrorComplianceFinding, InternalErrorComplianceId,
    InternalErrorComplianceReport, InternalErrorNodeClass, InternalErrorTypeGraphReport,
    InternalErrorTypeNode, InternalErrorTypeProbeId, WorkspaceInternalErrorChainSummary,
    build_workspace_internal_error_chain_summary,
};

use std::sync::LazyLock;

use crate::SourceLoader;
use crate::enricher::ERROR_IR_ENRICHERS;
use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, StaticEtiquette, StaticQualityEtiquette,
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
pub static INTERNAL_ERROR_CHAIN_ETIQUETTE: LazyLock<StaticQualityEtiquette> = LazyLock::new(|| {
    StaticQualityEtiquette::new(
        StaticEtiquette::new(
            "internal_error_chain",
            "Internal error chain",
            EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
            false,
            EtiquetteExplain::new(
                "Do this crate's error types form the parent / Kind / native-source architecture?",
                "Foreign-error etiquettes ask what leaks in. This one asks whether our error types are a place those leaks can land.",
                "Catalogs every type that implements Error under src/. By default a parent boxes an umbrella *Kind enum; every Kind variant holds a native source; native sources that wrap a foreign error keep it in source with owned file/line from Location::caller() and #[track_caller]. With [internal_error_chain] field_strategy = \"borrowed\", the parent holds &'a Kind, and every non-Copy field in an error type or its Kind is borrowed, including source and file (`&'static str`). Passing a caller-tracking constructor as a map_err function item loses the caller location; use ? or a closure. Feeds the hand-composed Error handling quality-report area.",
                "`[internal_error_chain] enabled = false` in cordial.toml opts out. `field_strategy = \"owned\" | \"borrowed\"` selects error field storage independently of `[antipatterns.static_refs]` (default owned).",
                vec![
                    EtiquetteRuleExplain::new(
                        "ERROR-CHAIN-COMPLIANCE-STRINGIFY-001",
                        "Converts an error to text without retaining its typed source",
                    ),
                    EtiquetteRuleExplain::new(
                        "ERROR-CHAIN-COMPLIANCE-DISCARD-TYPED-001",
                        "Discards a typed error",
                    ),
                    EtiquetteRuleExplain::new(
                        "ERROR-CHAIN-COMPLIANCE-SOURCE-SHAPE-001",
                        "Native source shape is wrong",
                    ),
                    EtiquetteRuleExplain::new(
                        "ERROR-CHAIN-COMPLIANCE-FIELD-STRATEGY-001",
                        "Borrowed error mode requires references for non-Copy fields and &'static str for file",
                    ),
                    EtiquetteRuleExplain::new(
                        "ERROR-CHAIN-COMPLIANCE-SOURCE-TRACK-CALLER-001",
                        "Native source missing #[track_caller]",
                    ),
                    EtiquetteRuleExplain::new(
                        "ERROR-CHAIN-COMPLIANCE-MAP-ERR-TRACK-CALLER-001",
                        "map_err function item loses the caller location; use ? or a closure",
                    ),
                    EtiquetteRuleExplain::new(
                        "ERROR-CHAIN-COMPLIANCE-ARCH-PARENT-001",
                        "Parent does not hold Kind using the configured field strategy",
                    ),
                    EtiquetteRuleExplain::new(
                        "ERROR-CHAIN-COMPLIANCE-ARCH-KIND-BOX-001",
                        "Kind storage does not match the configured field strategy",
                    ),
                    EtiquetteRuleExplain::new(
                        "ERROR-CHAIN-COMPLIANCE-ARCH-KIND-VARIANT-001",
                        "Kind variant is not a native source",
                    ),
                    EtiquetteRuleExplain::new(
                        "ERROR-CHAIN-COMPLIANCE-ARCH-KIND-FANOUT-001",
                        "Distinct literal call sites exceed [internal_error_chain] kind_fanout_max_call_sites (default 0) for a Kind variant or native-source constructor; review whether callers need distinct typed variants. Raise the maximum to allow matching sites.",
                    ),
                    EtiquetteRuleExplain::new(
                        "ERROR-CHAIN-COMPLIANCE-ARCH-ORPHAN-SOURCE-001",
                        "Orphan native source",
                    ),
                    EtiquetteRuleExplain::new(
                        "ERROR-CHAIN-INTERNAL-LEAF-001",
                        "Internal leaf in the type graph",
                    ),
                    EtiquetteRuleExplain::new(
                        "ERROR-CHAIN-INTERNAL-LINK-001",
                        "Internal link in the type graph",
                    ),
                    EtiquetteRuleExplain::new(
                        "ERROR-CHAIN-INTERNAL-NESTED-001",
                        "Nested internal error",
                    ),
                ],
            ),
        ),
        None,
    )
});
