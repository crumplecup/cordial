//! Trait-impl coverage against elicitation requirements.
//!
//! **What.** Measures whether target types implement the required elicitation
//! trait stack.
//!
//! **Why.** Elicitation coverage is a completeness inventory, not a source
//! lint. Types that wrap foreign values or sit on a tracked target need the
//! trait stack before they are done.
//!
//! **Flags.** Types that should implement `ElicitComplete` and prerequisite
//! traits, classified by [`ImplGapKind`]: missing local traits, ready for
//! `ElicitComplete`, feature-gated external, or externally blocked.
//!
//! **Ignores.** This etiquette does not judge source style. Foreign wrapping
//! belongs to `trenchcoat`; upstream mirror completeness belongs to `shadow`.
//!
//! **Outputs.** `{store}/findings/impl-coverage.checklist.md` plus coverage
//! and gap CSVs.
//!
//! **Config.** Run `cordial build rustdoc`, then `cordial coverage`. Requires
//! the `impl_coverage` / `elicitation` feature set. Register
//! [`IMPL_COVERAGE_ETIQUETTE`].

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
use crate::etiquette::{EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, StaticEtiquette};

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
pub static IMPL_COVERAGE_ETIQUETTE: StaticEtiquette = StaticEtiquette::new(
    "impl-coverage",
    "Impl coverage",
    EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
    true,
    EtiquetteExplain::new(
        "Do types implement the required elicitation traits?",
        "Elicitation coverage is a completeness inventory, not a source lint. Types that wrap foreign values or sit on a tracked target need the trait stack before they are done.",
        "From rustdoc JSON, finds types that should implement ElicitComplete (and prerequisites) and classifies gaps: missing our traits, ready for ElicitComplete, feature-gated external, or externally blocked. Needs cordial build rustdoc.",
        "`[impl-coverage] enabled = false` in cordial.toml.",
        &[EtiquetteRuleExplain::new(
            "IMPL-COVERAGE-GAP",
            "Type is missing required elicitation traits",
        )],
    ),
);
