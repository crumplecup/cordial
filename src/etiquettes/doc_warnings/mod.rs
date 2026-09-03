//! rustdoc diagnostics from `cargo doc`.
//!
//! **What.** Invokes `cargo doc --no-deps` and records each `rustdoc::*`
//! diagnostic (`DOC-WARNING-001`). rustc lints that happen to fire while
//! rustdoc compiles (`missing_docs`, `unused`, …) are dropped — check and
//! clippy already see those. The same span is kept once.
//!
//! **Why.** `cargo check` never runs rustdoc. Broken intra-doc links and
//! the rest of the `rustdoc::*` group only show up under `cargo doc`, which
//! is easy to skip locally until CI sets `RUSTDOCFLAGS=-D warnings`.
//!
//! **How to use.** Run `cordial quality` (feature `doc_warnings`, part of
//! `quality`). Skipped when `cargo` is not on `PATH` (`CORDIAL_CARGO` /
//! `CARGO` override the binary) or when the package is in
//! `[doc_warnings] skip_crates`. Artifacts:
//! `{store}/findings/doc-warnings.checklist.md`, `doc-warnings-summary.md`,
//! and CSV. Exceptions: `cordial exceptions show doc_warnings`.
//! Register [`DOC_WARNINGS_ETIQUETTE`] on a [`crate::Session`].
//!
//! Policy: `docs/planning/doc-warnings-etiquette.md`.

mod assessor;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::DocWarningAssessor;
pub use enricher::DocWarningInventoryEnricher;
pub use probe::DocWarningSiteProbe;
pub use reporter::{DocWarningChecklistReporter, DocWarningCsvReporter, DocWarningSummaryReporter};
pub use scan::{parse_doc_compiler_output, scan_crate_doc_warnings};
pub use types::{DocWarningRecord, DocWarningRuleId};

use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
    StaticQualityEtiquette, count_open_category,
};
use crate::objects::Finding;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

use tracing::instrument;

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static DOC_WARNING_INVENTORY: DocWarningInventoryEnricher = DocWarningInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static DOC_WARNING_PROBE: DocWarningSiteProbe = DocWarningSiteProbe;
static DOC_WARNING_ASSESSOR: DocWarningAssessor = DocWarningAssessor;
static DOC_WARNING_CSV: DocWarningCsvReporter = DocWarningCsvReporter;
static DOC_WARNING_CHECKLIST: DocWarningChecklistReporter = DocWarningChecklistReporter;
static DOC_WARNING_SUMMARY: DocWarningSummaryReporter = DocWarningSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] =
    &[&SCOPE_ENRICHER, &DOC_WARNING_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&'static dyn crate::Probe] = &[&DOC_WARNING_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&DOC_WARNING_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &DOC_WARNING_CSV,
    &DOC_WARNING_CHECKLIST,
    &DOC_WARNING_SUMMARY,
];

/// Built-in rustdoc-warning etiquette bundle.
pub static DOC_WARNINGS_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette::new(
    StaticEtiquette::new(
        "doc_warnings",
        "rustdoc warnings",
        EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
        false,
        EtiquetteExplain::new(
            "Does cargo doc emit rustdoc::* diagnostics rustc never sees?",
            "cargo check never runs rustdoc. Broken intra-doc links and the rest of the rustdoc::* group only show up under cargo doc, which is easy to skip locally until CI sets RUSTDOCFLAGS=-D warnings.",
            "Invokes cargo doc --no-deps and records each rustdoc::* diagnostic. rustc lints that fire while rustdoc compiles (missing_docs, unused, …) are dropped — check and clippy already see those. The same span is kept once. Skipped when cargo is missing from PATH or the package is in [doc_warnings] skip_crates.",
            "`[doc_warnings] enabled = false` in cordial.toml.",
            &[EtiquetteRuleExplain::new(
                "DOC-WARNING-001",
                "A rustdoc::* diagnostic from cargo doc",
            )],
        ),
    ),
    Some(QualityAreaSpec::new(
        "rustdoc warnings",
        "doc-warnings.checklist.md",
        "doc-warnings-summary.md",
        quality_area_compute,
    )),
);

#[instrument(level = "debug", skip(findings))]
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let doc_warnings = count_open_category(findings, "doc_warnings");
    (doc_warnings, format!("rustdoc warnings **{doc_warnings}**"))
}
