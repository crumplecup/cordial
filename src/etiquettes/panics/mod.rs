//! Panic and hard-fail sites in source.
//!
//! **What.** Inventories `panic!`, `unwrap`, `expect`, `unreachable!`, and
//! `compile_error!`. Each site is a [`PanicKind`] with a stable rule id
//! (`PANIC-SOURCE-*`). String literals that parse as Rust and contain those
//! APIs are treated as embedded fixture programs; keep samples under
//! `tests/fixtures/` or `tests/parity/`.
//!
//! **Why.** Abort sites are the first error-handling layer. Library code
//! should return the crate’s internal error type (preserving `source()`);
//! binaries and tests should surface through miette. Test `.unwrap` /
//! `.expect` (including `#[cfg(test)]` modules under `src/`) stay on the
//! checklist rather than becoming CSV-only inventory.
//!
//! **How to use.** Run `cordial quality` (feature `panics`, on by default).
//! Artifacts: `{store}/findings/panics.checklist.md`, `panics-summary.md`,
//! and CSV. Silence a site with `cordial exceptions show panics`. From a
//! library, register [`PANICS_ETIQUETTE`] on a [`crate::Session`].
//!
//! Policy: `docs/planning/error-handling-as-plugin.md`.

mod assessor;
mod enricher;
mod error_assertion;
mod kani_reach;
mod probe;
mod reporter;
mod scan;
mod types;
#[cfg(feature = "verus_ir")]
mod verus_reach;
#[cfg(not(feature = "verus_ir"))]
mod verus_recover;

pub use assessor::PanicAssessor;
pub use enricher::PanicInventoryEnricher;
pub use probe::PanicSiteProbe;
pub use reporter::{PanicChecklistReporter, PanicCsvReporter, PanicSummaryReporter};
pub use scan::{scan_crate_panics, scan_rust_source, scan_source_tree};
pub use types::PanicKind;

use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, StaticEtiquette, StaticQualityEtiquette,
};
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static PANIC_INVENTORY: PanicInventoryEnricher = PanicInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static PANIC_PROBE: PanicSiteProbe = PanicSiteProbe;
static PANIC_ASSESSOR: PanicAssessor = PanicAssessor;
static PANIC_CSV: PanicCsvReporter = PanicCsvReporter;
static PANIC_CHECKLIST: PanicChecklistReporter = PanicChecklistReporter;
static PANIC_SUMMARY: PanicSummaryReporter = PanicSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] =
    &[&SCOPE_ENRICHER, &PANIC_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&'static dyn crate::Probe] = &[&PANIC_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&PANIC_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[&PANIC_CSV, &PANIC_CHECKLIST, &PANIC_SUMMARY];

/// Built-in panics etiquette bundle.
pub static PANICS_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette::new(
    StaticEtiquette::new(
        "panics",
        "Panic sources",
        EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
        false,
        EtiquetteExplain::new(
            "Where does this crate abort?",
            "Abort sites are the first error-handling layer. Library code should return the crate's internal error type (preserving source()); binaries and tests should surface through miette.",
            "Inventories panic!, unwrap, expect, unreachable!, and compile_error!. String literals that parse as Rust and contain those APIs are scanned as embedded fixture programs; keep samples under tests/fixtures or tests/parity (path skip). Test unwrap/expect (including #[cfg(test)] modules under src/) stay on the checklist rather than becoming CSV-only inventory.",
            "`[panics] enabled = false` in cordial.toml.",
            &[
                EtiquetteRuleExplain::new("PANIC-SOURCE-PANIC", "`panic!` in source"),
                EtiquetteRuleExplain::new("PANIC-SOURCE-UNREACHABLE", "`unreachable!` in source"),
                EtiquetteRuleExplain::new("PANIC-SOURCE-EXPECT", "`.expect(...)` in source"),
                EtiquetteRuleExplain::new("PANIC-SOURCE-UNWRAP", "`.unwrap()` in source"),
                EtiquetteRuleExplain::new(
                    "PANIC-SOURCE-COMPILE-ERROR",
                    "`compile_error!` in source",
                ),
            ],
        ),
    ),
    None,
);
