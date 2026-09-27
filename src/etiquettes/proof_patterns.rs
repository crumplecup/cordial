//! Soundness and proof-visibility patterns inside `verus! { .. }` blocks.
//!
//! **What.** Inventories trusted proof shortcuts and implicit proof
//! dependencies extracted from a genuine `verus_syn` parse.
//!
//! **Why.** A `verus!` function signature says what it proves; these signals
//! say how much is actually checked versus trusted. Broadcast lemmas also hide
//! dependency surface from the proof body. Ordinary cargo, Clippy, and Verus
//! warnings do not report these forms.
//!
//! **Flags.** `assume(..)`, `admit()`, `#[verifier::external_body]`,
//! `uninterp spec fn`, `axiom fn`, and `broadcast proof fn`. Each site is a
//! [`ProofPatternKind`] with a stable `PROOF-PATTERN-*` rule id.
//!
//! **Ignores.** This etiquette does not prove obligations or judge whether a
//! trusted escape hatch is justified. It makes the proof surface visible for
//! review.
//!
//! **Outputs.** `{store}/findings/proof-patterns.checklist.md`,
//! `proof-patterns-summary.md`, and CSV.
//!
//! **Config.** `[proof_patterns] enabled = false` opts out in
//! `cordial.toml`. Requires `verus_ir`. Exceptions are managed with
//! `cordial exceptions show proof_patterns`. Register
//! [`PROOF_PATTERNS_ETIQUETTE`]. Policy:
//! `docs/planning/proof-patterns-etiquette.md`.

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

use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
    StaticQualityEtiquette, count_open_rule,
};
use crate::objects::Finding;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

use tracing::instrument;

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
pub static PROOF_PATTERNS_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette::new(
    StaticEtiquette::new(
        "proof_patterns",
        "Proof patterns",
        EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
        false,
        EtiquetteExplain::new(
            "Which verus! functions are trusted rather than proven, or apply themselves invisibly?",
            "A verus! function's signature says what it proves; these signals say how much of that is actually checked versus trusted, and (for broadcast) how much of a proof's real dependency surface is invisible. Verus accepts every one of these forms without complaint.",
            "Inventories assume, admit, #[verifier::external_body], uninterp spec fn, axiom fn (trusted rather than proven), and broadcast proof fn. Requires verus_ir. Ordinary cargo check / clippy / verus_warnings never see these.",
            "`[proof_patterns] enabled = false` in cordial.toml.",
            &[
                EtiquetteRuleExplain::new(
                    "PROOF-PATTERN-ASSUME",
                    "`assume(...)` in a verus! function",
                ),
                EtiquetteRuleExplain::new("PROOF-PATTERN-ADMIT", "`admit()` in a verus! function"),
                EtiquetteRuleExplain::new(
                    "PROOF-PATTERN-EXTERNAL-BODY",
                    "`#[verifier::external_body]`",
                ),
                EtiquetteRuleExplain::new("PROOF-PATTERN-UNINTERP", "`uninterp spec fn`"),
                EtiquetteRuleExplain::new("PROOF-PATTERN-AXIOM", "`axiom fn`"),
                EtiquetteRuleExplain::new("PROOF-PATTERN-BROADCAST", "`broadcast proof fn`"),
            ],
        ),
    ),
    Some(QualityAreaSpec::new(
        "Proof patterns",
        "proof-patterns.checklist.md",
        "proof-patterns-summary.md",
        quality_area_compute,
    )),
);

#[instrument(level = "debug", skip(findings))]
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let assume = count_open_rule(findings, "PROOF-PATTERN-ASSUME");
    let admit = count_open_rule(findings, "PROOF-PATTERN-ADMIT");
    let external_body = count_open_rule(findings, "PROOF-PATTERN-EXTERNAL-BODY");
    let uninterp = count_open_rule(findings, "PROOF-PATTERN-UNINTERP");
    let axiom = count_open_rule(findings, "PROOF-PATTERN-AXIOM");
    let broadcast = count_open_rule(findings, "PROOF-PATTERN-BROADCAST");
    let total = assume + admit + external_body + uninterp + axiom + broadcast;
    let detail = format!(
        "assume **{assume}**, admit **{admit}**, external_body **{external_body}**, \
         uninterp **{uninterp}**, axiom **{axiom}**, broadcast **{broadcast}**"
    );
    (total, detail)
}
