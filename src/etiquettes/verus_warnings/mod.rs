//! Warnings from the Verus rustc fork.
//!
//! **What.** Invokes `verus` on crates that are Verus compilation units
//! (`*_verus` or a `vstd` / `verus_builtin` dependency) and records each
//! `warning:` diagnostic (`VERUS-WARNING-001`). Rustc summary lines
//! (`N warnings emitted`) are dropped; the same span is kept once.
//!
//! **Why.** Verus is a different compiler. It fires diagnostics rustc and
//! clippy never see, and it has no deny-warnings flag. Catch them in post
//! so a clean `cargo clippy -D warnings` cannot hide a Verus warning.
//!
//! **How to use.** Run `cordial quality` (feature `verus_warnings`, part of
//! `quality`). The crate is skipped when it is not a Verus target or when
//! `verus` is not on `PATH` (`CORDIAL_VERUS` / `VERUS` / `VERUS_PATH`
//! override the binary). Artifacts: `{store}/findings/verus-warnings.checklist.md`,
//! `verus-warnings-summary.md`, and CSV. Exceptions:
//! `cordial exceptions show verus_warnings`.
//! Register [`VERUS_WARNINGS_ETIQUETTE`] on a [`crate::Session`].
//!
//! Policy: `docs/planning/verus-warnings-etiquette.md`.

mod assessor;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::VerusWarningAssessor;
pub use enricher::VerusWarningInventoryEnricher;
pub use probe::VerusWarningSiteProbe;
pub use reporter::{
    VerusWarningChecklistReporter, VerusWarningCsvReporter, VerusWarningSummaryReporter,
};
pub use scan::{crate_is_verus_target, parse_verus_compiler_output, scan_crate_verus_warnings};
pub use types::{VerusWarningRecord, VerusWarningRuleId};

use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
    StaticQualityEtiquette, count_open_category,
};
use crate::objects::Finding;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

use tracing::instrument;

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static VERUS_WARNING_INVENTORY: VerusWarningInventoryEnricher = VerusWarningInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static VERUS_WARNING_PROBE: VerusWarningSiteProbe = VerusWarningSiteProbe;
static VERUS_WARNING_ASSESSOR: VerusWarningAssessor = VerusWarningAssessor;
static VERUS_WARNING_CSV: VerusWarningCsvReporter = VerusWarningCsvReporter;
static VERUS_WARNING_CHECKLIST: VerusWarningChecklistReporter = VerusWarningChecklistReporter;
static VERUS_WARNING_SUMMARY: VerusWarningSummaryReporter = VerusWarningSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] = &[
    &SCOPE_ENRICHER,
    &VERUS_WARNING_INVENTORY,
    &ATTRIBUTE_ENRICHER,
];
static PROBES: &[&'static dyn crate::Probe] = &[&VERUS_WARNING_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&VERUS_WARNING_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &VERUS_WARNING_CSV,
    &VERUS_WARNING_CHECKLIST,
    &VERUS_WARNING_SUMMARY,
];

/// Built-in Verus compiler-warning etiquette bundle.
pub static VERUS_WARNINGS_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette::new(
    StaticEtiquette::new(
        "verus_warnings",
        "Verus compiler warnings",
        EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
        false,
        EtiquetteExplain::new(
            "Does the Verus rustc fork emit warnings this crate's rustc never sees?",
            "Verus is a different compiler. It fires diagnostics rustc and clippy never see, and it has no deny-warnings flag.",
            "Invokes verus on crates that are Verus compilation units (*_verus or a vstd / verus_builtin dependency) and records each warning: diagnostic. Summary lines are dropped; the same span is kept once. Skipped when the crate is not a Verus target or verus is not on PATH.",
            "`[verus_warnings] enabled = false` in cordial.toml.",
            &[EtiquetteRuleExplain::new(
                "VERUS-WARNING-001",
                "A warning: line from the Verus rustc fork",
            )],
        ),
    ),
    Some(QualityAreaSpec::new(
        "Verus compiler warnings",
        "verus-warnings.checklist.md",
        "verus-warnings-summary.md",
        quality_area_compute,
    )),
);

#[instrument(level = "debug", skip(findings))]
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let verus_warnings = count_open_category(findings, "verus_warnings");
    (
        verus_warnings,
        format!("Verus compiler warnings **{verus_warnings}**"),
    )
}
