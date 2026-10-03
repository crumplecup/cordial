//! Arrangement of items in a file.
//!
//! **What.** Keeps trait contracts in the leading declaration block, and
//! keeps `lib.rs` / `mod.rs` as a table of contents.
//!
//! **Why.** Contracts belong at the top of the file. A crate or directory
//! index is visibility and re-exports; types and functions live in a named
//! sibling (`foo.rs`), not in the barrel.
//!
//! **Flags.** A trait definition after the leading trait block as
//! `PAGEANTRY-TRAIT-001`. A type or function declaration in `lib.rs` or
//! `mod.rs` as `PAGEANTRY-BARREL-001`. A proc-macro entry point in a barrel
//! whose body is longer than the shim limit as `PAGEANTRY-BARREL-SHIM-001`.
//!
//! **Ignores.** `use`, `extern crate`, and `mod` declarations are treated as
//! the file header (and as the only legal items in a barrel file). Several
//! traits in a row just below that header are valid. `#[cfg(test)]` items
//! are skipped. `#[proc_macro]`, `#[proc_macro_derive]`, and
//! `#[proc_macro_attribute]` functions, which rustc requires at a
//! proc-macro crate root, are allowed in a barrel as long as they stay
//! shims (short bodies that delegate to a named file).
//!
//! **Outputs.** `{store}/findings/pageantry.checklist.md`,
//! `pageantry-summary.md`, and CSV.
//!
//! **Config.** `[pageantry]` in `cordial.toml`: `enabled`, a per-rule
//! switch (`trait_block`, `barrel`, `barrel_shim`), and `max_shim_lines`.
//! Register [`PAGEANTRY_ETIQUETTE`] on a [`crate::Session`]. Policy:
//! `docs/planning/pageantry-etiquette.md`.

mod assessor;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod shim;
mod types;

pub use assessor::PageantryAssessor;
pub use enricher::PageantryInventoryEnricher;
pub use probe::PageantrySiteProbe;
pub use reporter::{PageantryChecklistReporter, PageantryCsvReporter, PageantrySummaryReporter};
pub use scan::{scan_crate_pageantry, scan_rust_source};
pub use types::PageantryRuleId;

use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
    StaticQualityEtiquette, count_open_rule,
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
pub static PAGEANTRY_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette::new(
    StaticEtiquette::new(
        "pageantry",
        "Pageantry",
        EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
        false,
        EtiquetteExplain::new(
            "Are traits at the top of the file, and are lib.rs / mod.rs only modules and re-exports?",
            "Contracts belong at the top of the file. A crate or directory index is a table of contents: visibility and re-exports, not types or functions.",
            "Walks each file and inline mod item list in source order. use / extern crate / mod are header. A run of traits at the front is fine. After any other item (struct, enum, impl, fn, …), every later trait is PAGEANTRY-TRAIT-001. Files named lib.rs or mod.rs may contain only those header items; any other item is PAGEANTRY-BARREL-001. #[cfg(test)] items are skipped, and so are #[proc_macro], #[proc_macro_derive], and #[proc_macro_attribute] functions are exempt from BARREL-001 because rustc requires them at a proc-macro crate root, but their bodies must stay shims: more than max_shim_lines lines between the braces is PAGEANTRY-BARREL-SHIM-001 (move the logic to a named file and delegate).",
            "`[pageantry] enabled = false` in cordial.toml turns the etiquette off. `trait_block`, `barrel`, and `barrel_shim` (all default true) turn single rules off; `max_shim_lines` (default 8) sets the shim size limit.",
            &[
                EtiquetteRuleExplain::new(
                    "PAGEANTRY-TRAIT-001",
                    "A trait defined after the leading trait block has ended",
                ),
                EtiquetteRuleExplain::new(
                    "PAGEANTRY-BARREL-001",
                    "A type or function declaration in lib.rs or mod.rs",
                ),
                EtiquetteRuleExplain::new(
                    "PAGEANTRY-BARREL-SHIM-001",
                    "A proc-macro entry point in lib.rs or mod.rs whose body is longer than max_shim_lines",
                ),
            ],
        ),
    ),
    Some(QualityAreaSpec::new(
        "Pageantry",
        "pageantry.checklist.md",
        "pageantry-summary.md",
        quality_area_compute,
    )),
);

#[instrument(level = "debug", skip(findings))]
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let traits = count_open_rule(findings, "PAGEANTRY-TRAIT-001");
    let barrels = count_open_rule(findings, "PAGEANTRY-BARREL-001");
    let shims = count_open_rule(findings, "PAGEANTRY-BARREL-SHIM-001");
    (
        traits + barrels + shims,
        format!(
            "misplaced traits **{traits}**, barrel items **{barrels}**, oversized shims **{shims}**"
        ),
    )
}
