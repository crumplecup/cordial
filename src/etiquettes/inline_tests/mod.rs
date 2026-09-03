//! Tests mixed into library source.
//!
//! **What.** Flags `#[cfg(test)]` modules and leftover `#[test]` functions
//! under `src/` (`INLINE-TEST-MOD`, `INLINE-TEST-CFG`, `INLINE-TEST-FN`).
//! Crate `tests/` is the destination, not a finding.
//!
//! **Why.** Inline tests hide cases from readers of the library and mix
//! test-only helpers into production modules. Integration tests in `tests/`
//! stay visible and match cordial’s own layout rule.
//!
//! **How to use.** Run `cordial quality` (feature `inline_tests`, part of
//! `quality`). Artifacts: `{store}/findings/inline-tests.checklist.md`,
//! `inline-tests-summary.md`, and CSV. Exceptions: `cordial exceptions show inline_tests`.
//! Register [`INLINE_TESTS_ETIQUETTE`] on a [`crate::Session`].
//!
//! Policy: `docs/planning/inline-tests-etiquette.md`.

mod assessor;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::InlineTestAssessor;
pub use enricher::InlineTestInventoryEnricher;
pub use probe::InlineTestSiteProbe;
pub use reporter::{InlineTestChecklistReporter, InlineTestCsvReporter, InlineTestSummaryReporter};
pub use scan::{scan_crate_inline_tests, scan_rust_source};
pub use types::InlineTestRuleId;

use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
    StaticQualityEtiquette, count_open_category,
};
use crate::objects::Finding;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

use tracing::instrument;

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static INLINE_TEST_INVENTORY: InlineTestInventoryEnricher = InlineTestInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static INLINE_TEST_PROBE: InlineTestSiteProbe = InlineTestSiteProbe;
static INLINE_TEST_ASSESSOR: InlineTestAssessor = InlineTestAssessor;
static INLINE_TEST_CSV: InlineTestCsvReporter = InlineTestCsvReporter;
static INLINE_TEST_CHECKLIST: InlineTestChecklistReporter = InlineTestChecklistReporter;
static INLINE_TEST_SUMMARY: InlineTestSummaryReporter = InlineTestSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] =
    &[&SCOPE_ENRICHER, &INLINE_TEST_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&'static dyn crate::Probe] = &[&INLINE_TEST_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&INLINE_TEST_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &INLINE_TEST_CSV,
    &INLINE_TEST_CHECKLIST,
    &INLINE_TEST_SUMMARY,
];

/// Built-in inline-tests etiquette bundle.
pub static INLINE_TESTS_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette::new(
    StaticEtiquette::new(
        "inline_tests",
        "Inline tests",
        EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
        false,
        EtiquetteExplain::new(
            "Are tests mixed into src/ instead of tests/?",
            "Inline tests hide cases from readers of the library and mix test-only helpers into production modules.",
            "Flags #[cfg(test)] modules and leftover #[test] functions under src/. Crate tests/ is the destination, not a finding.",
            "`[inline_tests] enabled = false` in cordial.toml.",
            &[
                EtiquetteRuleExplain::new("INLINE-TEST-MOD", "`#[cfg(test)]` module under src/"),
                EtiquetteRuleExplain::new("INLINE-TEST-CFG", "`#[cfg(test)]` item under src/"),
                EtiquetteRuleExplain::new("INLINE-TEST-FN", "`#[test]` function under src/"),
            ],
        ),
    ),
    Some(QualityAreaSpec::new(
        "Inline tests",
        "inline-tests.checklist.md",
        "inline-tests-summary.md",
        quality_area_compute,
    )),
);

#[instrument(level = "debug", skip(findings))]
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let inline_tests = count_open_category(findings, "inline_tests");
    (
        inline_tests,
        format!("tests under `src/` **{inline_tests}**"),
    )
}
