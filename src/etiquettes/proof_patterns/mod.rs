//! Soundness and proof-visibility patterns inside `verus! { .. }` blocks.
//!
//! **What.** Inventories six real, local signals `verus_ir` already
//! extracts from a genuine `verus_syn` parse: `assume(..)`, `admit()`,
//! `#[verifier::external_body]`, `uninterp spec fn`, `axiom fn` (each a
//! function trusted rather than proven -- `VerusFnFacts::
//! is_trusted_not_proven()`), and `broadcast proof fn` (a lemma applied
//! automatically to every proof in scope, invisibly, at every call
//! site). Each site is a [`ProofPatternKind`] with a stable rule id
//! (`PROOF-PATTERN-*`).
//!
//! **Why.** A `verus!` function's signature says what it proves; these
//! six signals say how much of that is actually checked versus trusted,
//! and (for `broadcast`) how much of a proof's real dependency surface
//! is invisible from its own body. None of this shows up in an ordinary
//! `cargo check`/`clippy` pass, or even in `verus`'s own compiler
//! warnings (see [verus_warnings](../verus_warnings/index.html)) --
//! Verus accepts every one of these forms without complaint, by design.
//!
//! **How to use.** Run `cordial quality` (feature `proof_patterns`,
//! requires `verus_ir`, part of `quality`). Artifacts:
//! `{store}/findings/proof-patterns.checklist.md`,
//! `proof-patterns-summary.md`, and CSV. Silence a site with `cordial
//! exceptions show proof_patterns`. From a library, register
//! [`PROOF_PATTERNS_ETIQUETTE`] on a [`crate::Session`].
//!
//! Policy: `docs/planning/proof-patterns-etiquette.md`.

mod assessor;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::ProofPatternAssessor;
pub use enricher::ProofPatternInventoryEnricher;
pub use probe::ProofPatternSiteProbe;
pub use reporter::{
    ProofPatternChecklistReporter, ProofPatternCsvReporter, ProofPatternSummaryReporter,
};
pub use scan::scan_crate_proof_patterns;
pub use types::ProofPatternKind;

use crate::etiquette::StaticEtiquette;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static PROOF_PATTERN_INVENTORY: ProofPatternInventoryEnricher = ProofPatternInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static PROOF_PATTERN_PROBE: ProofPatternSiteProbe = ProofPatternSiteProbe;
static PROOF_PATTERN_ASSESSOR: ProofPatternAssessor = ProofPatternAssessor;
static PROOF_PATTERN_CSV: ProofPatternCsvReporter = ProofPatternCsvReporter;
static PROOF_PATTERN_CHECKLIST: ProofPatternChecklistReporter = ProofPatternChecklistReporter;
static PROOF_PATTERN_SUMMARY: ProofPatternSummaryReporter = ProofPatternSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] = &[
    &SCOPE_ENRICHER,
    &PROOF_PATTERN_INVENTORY,
    &ATTRIBUTE_ENRICHER,
];
static PROBES: &[&'static dyn crate::Probe] = &[&PROOF_PATTERN_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&PROOF_PATTERN_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &PROOF_PATTERN_CSV,
    &PROOF_PATTERN_CHECKLIST,
    &PROOF_PATTERN_SUMMARY,
];

/// Built-in proof-patterns etiquette bundle.
pub static PROOF_PATTERNS_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "proof_patterns",
    name: "Proof patterns",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: None,
    reporters: REPORTERS,
    is_coverage: false,
};
