//! Trait-impl coverage against elicitation requirements.
//!
//! **What.** From rustdoc JSON, finds types that should implement
//! `ElicitComplete` (and prerequisite traits) and classifies gaps
//! ([`ImplGapKind`]): missing our traits, ready for `ElicitComplete`,
//! feature-gated external, or externally blocked.
//!
//! **Why.** Elicitation coverage is a completeness inventory, not a source
//! lint. Types that wrap foreign values or sit on a tracked target need the
//! trait stack before they are “done.”
//!
//! **How to use.** `cordial build rustdoc`, then `cordial coverage` (feature
//! `impl_coverage` / `elicitation`). Artifacts:
//! `{store}/findings/impl-coverage.checklist.md` plus coverage/gaps CSVs.
//! Register [`IMPL_COVERAGE_ETIQUETTE`].

mod assessor;
mod gap_classify;
mod node_context;
mod probe;
mod reporter;
mod types;

pub use assessor::ImplGapAssessor;
pub use gap_classify::{ImplGapAssessment, assess_impl_gap};
pub use probe::MissingPrereqProbe;
pub use reporter::{ImplChecklistReporter, ImplCoverageCsvReporter, ImplGapsCsvReporter};
pub use types::ImplGapKind;

use crate::RustdocLoader;
use crate::enricher::{
    FeatureProbeEnricher, ProofHarnessEnricher, TraitImplEnricher, WrapperCoverageEnricher,
};
use crate::etiquette::{EtiquetteExplain, EtiquetteRuleExplain, StaticEtiquette};

static RUSTDOC_LOADER: RustdocLoader = RustdocLoader;
static TRAIT_IMPL: TraitImplEnricher = TraitImplEnricher;
static FEATURE_PROBE: FeatureProbeEnricher = FeatureProbeEnricher;
static WRAPPER_COVERAGE: WrapperCoverageEnricher = WrapperCoverageEnricher;
static PROOF_HARNESS: ProofHarnessEnricher = ProofHarnessEnricher;
static MISSING_PROBE: MissingPrereqProbe = MissingPrereqProbe;
static IMPL_ASSESSOR: ImplGapAssessor = ImplGapAssessor;
static IMPL_CSV: ImplCoverageCsvReporter = ImplCoverageCsvReporter;
static IMPL_GAPS_CSV: ImplGapsCsvReporter = ImplGapsCsvReporter;
static IMPL_CHECKLIST: ImplChecklistReporter = ImplChecklistReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&RUSTDOC_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] = &[
    &TRAIT_IMPL,
    &FEATURE_PROBE,
    &WRAPPER_COVERAGE,
    &PROOF_HARNESS,
];
static PROBES: &[&'static dyn crate::Probe] = &[&MISSING_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&IMPL_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[&IMPL_CSV, &IMPL_GAPS_CSV, &IMPL_CHECKLIST];

/// Built-in trait impl coverage etiquette bundle.
pub static IMPL_COVERAGE_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "impl-coverage",
    name: "Impl coverage",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: None,
    reporters: REPORTERS,
    is_coverage: true,
    explain: EtiquetteExplain {
        summary: "Do types implement the required elicitation traits?",
        why: "Elicitation coverage is a completeness inventory, not a source lint. Types that wrap foreign values or sit on a tracked target need the trait stack before they are done.",
        logic: "From rustdoc JSON, finds types that should implement ElicitComplete (and prerequisites) and classifies gaps: missing our traits, ready for ElicitComplete, feature-gated external, or externally blocked. Needs cordial build rustdoc.",
        opt_out: "`[impl-coverage] enabled = false` in cordial.toml.",
        rules: &[EtiquetteRuleExplain {
            id: "IMPL-COVERAGE-GAP",
            summary: "Type is missing required elicitation traits",
        }],
    },
};
