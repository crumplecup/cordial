//! Arrangement of types in a file (pageantry).
//!
//! **What.** First rule: a trait definition after the leading trait
//! block (`PAGEANTRY-TRAIT-001`). Several traits in a row just below
//! `use` / `extern crate` / `mod` are fine. A trait after a type (or
//! any other body item) is not.
//!
//! **Why.** Contracts belong at the top of the file. A trait that
//! appears once types have already started is ceremony in the middle of
//! the show — harder to find, and it usually means the file grew in
//! the order the author thought of things rather than the order a
//! reader needs.
//!
//! **How to use.** Run `cordial quality` (feature `pageantry`, part of
//! `quality`). Artifacts: `{store}/findings/pageantry.checklist.md`,
//! `pageantry-summary.md`, and CSV. Opt out:
//! `[pageantry] enabled = false` in `cordial.toml`. Register
//! [`PAGEANTRY_ETIQUETTE`] on a [`crate::Session`].
//!
//! Policy: `docs/planning/pageantry-etiquette.md`.

mod assessor;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::PageantryAssessor;
pub use enricher::PageantryInventoryEnricher;
pub use probe::PageantrySiteProbe;
pub use reporter::{PageantryChecklistReporter, PageantryCsvReporter, PageantrySummaryReporter};
pub use scan::{scan_crate_pageantry, scan_rust_source};
pub use types::PageantryRuleId;

use crate::etiquette::{
    EtiquetteExplain, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
    StaticQualityEtiquette, count_open_category,
};
use crate::objects::Finding;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

use tracing::instrument;

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static PAGEANTRY_INVENTORY: PageantryInventoryEnricher = PageantryInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static PAGEANTRY_PROBE: PageantrySiteProbe = PageantrySiteProbe;
static PAGEANTRY_ASSESSOR: PageantryAssessor = PageantryAssessor;
static PAGEANTRY_CSV: PageantryCsvReporter = PageantryCsvReporter;
static PAGEANTRY_CHECKLIST: PageantryChecklistReporter = PageantryChecklistReporter;
static PAGEANTRY_SUMMARY: PageantrySummaryReporter = PageantrySummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] =
    &[&SCOPE_ENRICHER, &PAGEANTRY_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&'static dyn crate::Probe] = &[&PAGEANTRY_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&PAGEANTRY_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] =
    &[&PAGEANTRY_CSV, &PAGEANTRY_CHECKLIST, &PAGEANTRY_SUMMARY];

/// Built-in pageantry etiquette bundle.
pub static PAGEANTRY_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette {
    etiquette: StaticEtiquette {
        id: "pageantry",
        name: "Pageantry",
        loaders: LOADERS,
        enrichers: ENRICHERS,
        probes: PROBES,
        assessors: ASSESSORS,
        workspace_assessors: None,
        reporters: REPORTERS,
        is_coverage: false,
        explain: EtiquetteExplain {
            summary: "Are traits defined in a leading block just below the import / mod header?",
            why: "Contracts belong at the top of the file. A trait after types have already started is ceremony in the middle of the show.",
            logic: "Walks each file and inline mod item list in source order. use / extern crate / mod are header. A run of traits at the front is fine. After any other item (struct, enum, impl, fn, …), every later trait is PAGEANTRY-TRAIT-001. #[cfg(test)] items are skipped.",
            opt_out: "`[pageantry] enabled = false` in cordial.toml.",
            rules: &[EtiquetteRuleExplain {
                id: "PAGEANTRY-TRAIT-001",
                summary: "A trait defined after the leading trait block has ended",
            }],
        },
    },
    quality_area: Some(QualityAreaSpec {
        title: "Pageantry",
        checklist: "pageantry.checklist.md",
        summary: "pageantry-summary.md",
        compute: quality_area_compute,
    }),
};

#[instrument(level = "debug", skip(findings))]
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let pageantry = count_open_category(findings, "pageantry");
    (pageantry, format!("misplaced traits **{pageantry}**"))
}
