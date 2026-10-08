//! Compiler warnings that only fire under some feature combinations.
//!
//! **What.** Runs `cargo hack check --feature-powerset` and records every
//! warning that fires under some feature combinations and not others, with
//! the `cfg` gate that would silence it.
//!
//! **Why.** `cargo check` and clippy compile one feature set. An import used
//! only by code behind `feature = "x"`, or a helper whose single consumer is
//! gated off, warns under the other combinations and nobody sees it.
//! `cargo hack` only fails on errors, so its warnings scroll past.
//!
//! **Flags.** `FEATURE-WARNING-001` for `unused_*` lints and
//! `FEATURE-WARNING-002` for `dead_code`; one finding per `(file, line,
//! lint)`, grouped in the checklist by suggested gate.
//!
//! **Ignores.** Warnings that fire in every combination (an ordinary warning
//! clippy already sees), unless `include_universal` is set. Combinations that
//! fail to compile. Crates in `[feature_warnings] skip_crates`. Skipped when
//! `cargo-hack` is not installed.
//!
//! **Outputs.** `{store}/findings/feature-warnings.checklist.md`,
//! `feature-warnings-summary.md`, and CSV.
//!
//! **Config.** `[feature_warnings]` owns `enabled` (off by default: the
//! powerset is many cold checks), `depth`, `exclude_features`,
//! `group_features`, `include_universal`, and `skip_crates`. Register
//! [`FEATURE_WARNINGS_ETIQUETTE`]. Policy:
//! `docs/planning/feature-warnings-etiquette.md`.

mod assessor;
mod enricher;
mod gate;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::FeatureWarningAssessor;
pub use enricher::FeatureWarningInventoryEnricher;
pub use gate::{FeatureSet, Gate, suggest_gate};
pub use probe::FeatureWarningSiteProbe;
pub use reporter::{
    FeatureWarningChecklistReporter, FeatureWarningCsvReporter, FeatureWarningSummaryReporter,
};
pub use scan::{HackRun, parse_cargo_hack_output, records_from_run, scan_crate_feature_warnings};
pub use types::{FeatureWarningRecord, FeatureWarningRuleId};

use std::sync::LazyLock;

use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
    StaticQualityEtiquette, count_open_category,
};
use crate::objects::Finding;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

use tracing::instrument;

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static FEATURE_WARNING_INVENTORY: FeatureWarningInventoryEnricher = FeatureWarningInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static FEATURE_WARNING_PROBE: FeatureWarningSiteProbe = FeatureWarningSiteProbe;
static FEATURE_WARNING_ASSESSOR: FeatureWarningAssessor = FeatureWarningAssessor;
static FEATURE_WARNING_CSV: FeatureWarningCsvReporter = FeatureWarningCsvReporter;
static FEATURE_WARNING_CHECKLIST: FeatureWarningChecklistReporter = FeatureWarningChecklistReporter;
static FEATURE_WARNING_SUMMARY: FeatureWarningSummaryReporter = FeatureWarningSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] = &[
    &SCOPE_ENRICHER,
    &FEATURE_WARNING_INVENTORY,
    &ATTRIBUTE_ENRICHER,
];
static PROBES: &[&'static dyn crate::Probe] = &[&FEATURE_WARNING_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&FEATURE_WARNING_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &FEATURE_WARNING_CSV,
    &FEATURE_WARNING_CHECKLIST,
    &FEATURE_WARNING_SUMMARY,
];

/// Built-in feature-powerset warning etiquette bundle.
pub static FEATURE_WARNINGS_ETIQUETTE: LazyLock<StaticQualityEtiquette> = LazyLock::new(|| {
    StaticQualityEtiquette::new(
        StaticEtiquette::new(
            "feature_warnings",
            "feature warnings",
            EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
            false,
            EtiquetteExplain::new(
                "Does a warning fire under some feature combinations that a normal build never compiles?",
                "cargo check and clippy compile one feature set. An import used only by code behind a feature, or a helper whose single consumer is gated off, warns under the other combinations and nobody sees it. cargo hack only fails on errors, so its warnings scroll past.",
                "Runs cargo hack check --feature-powerset and records each warning that fires in some combinations and not others, with the cfg gate that would silence it (an any(...) over the features that use the item, an all(...) when it needs several, or a note that no single gate fits). Warnings that fire everywhere are ordinary warnings and are left to cargo check. Skipped when cargo-hack is missing, and off unless [feature_warnings] enabled = true.",
                "`[feature_warnings] enabled = false` in cordial.toml (the default).",
                vec![
                    EtiquetteRuleExplain::new(
                        "FEATURE-WARNING-001",
                        "An unused import or binding under some feature combinations",
                    ),
                    EtiquetteRuleExplain::new(
                        "FEATURE-WARNING-002",
                        "Dead code under some feature combinations",
                    ),
                ],
            ),
        ),
        Some(QualityAreaSpec::new(
            "feature warnings",
            "feature-warnings.checklist.md",
            "feature-warnings-summary.md",
            quality_area_compute,
        )),
    )
});

#[instrument(level = "debug", skip(findings))]
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let feature_warnings = count_open_category(findings, "feature_warnings");
    (
        feature_warnings,
        format!("feature warnings **{feature_warnings}**"),
    )
}
