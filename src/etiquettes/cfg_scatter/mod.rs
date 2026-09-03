//! Scattered `#[cfg]` predicates that belong on a `mod`.
//!
//! **What.** Flags a `#[cfg(...)]` predicate copied across multiple item
//! kinds in one file (functions, impls, imports, …), or repeated many times
//! on one kind. `#[cfg]` on a `mod` is the recommended shape and is never
//! scanned. Field- and variant-only gating is never flagged
//! ([`CfgSiteKind`]).
//!
//! **Why.** Copy-pasted feature lists on free-standing items are a “this
//! logic is its own module” signal. Gating a field that holds a
//! feature-gated type is often unavoidable and is not that signal.
//!
//! **How to use.** Run `cordial quality` (feature `cfg_scatter`). Thresholds
//! live under `[cfg_scatter]` in `cordial.toml` (`min_distinct_kinds`,
//! `min_occurrences`). Artifacts: `{store}/findings/cfg-scatter.checklist.md`
//! and `cfg-scatter-summary.md`. Register [`CFG_SCATTER_ETIQUETTE`].
//!
//! Policy: `docs/planning/cfg-scatter-etiquette.md`.

mod assessor;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::CfgScatterAssessor;
pub use enricher::CfgScatterInventoryEnricher;
pub use probe::CfgScatterSiteProbe;
pub use reporter::{CfgScatterChecklistReporter, CfgScatterCsvReporter, CfgScatterSummaryReporter};
pub use scan::scan_rust_source;
pub use types::CfgSiteKind;

use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
    StaticQualityEtiquette, count_open_category,
};
use crate::objects::Finding;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

use tracing::instrument;

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static CFG_SCATTER_INVENTORY: CfgScatterInventoryEnricher = CfgScatterInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static CFG_SCATTER_PROBE: CfgScatterSiteProbe = CfgScatterSiteProbe;
static CFG_SCATTER_ASSESSOR: CfgScatterAssessor = CfgScatterAssessor;
static CFG_SCATTER_CSV: CfgScatterCsvReporter = CfgScatterCsvReporter;
static CFG_SCATTER_CHECKLIST: CfgScatterChecklistReporter = CfgScatterChecklistReporter;
static CFG_SCATTER_SUMMARY: CfgScatterSummaryReporter = CfgScatterSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] =
    &[&SCOPE_ENRICHER, &CFG_SCATTER_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&'static dyn crate::Probe] = &[&CFG_SCATTER_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&CFG_SCATTER_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &CFG_SCATTER_CSV,
    &CFG_SCATTER_CHECKLIST,
    &CFG_SCATTER_SUMMARY,
];

/// Built-in cfg-scatter etiquette bundle: flags a `#[cfg(...)]` predicate
/// repeated across multiple item kinds in one file (functions, impls,
/// imports, …) that would be clearer as a single `#[cfg]`-gated `mod`.
/// Field/variant-only gating is never flagged — see [`CfgSiteKind`] docs.
pub static CFG_SCATTER_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette::new(
    StaticEtiquette::new(
        "cfg_scatter",
        "Scattered cfg predicates",
        EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
        false,
        EtiquetteExplain::new(
            "Is the same #[cfg] copied across item kinds instead of a gated mod?",
            "Copy-pasted feature lists on free-standing items are a “this logic is its own module” signal. Gating a field that holds a feature-gated type is often unavoidable and is not that signal.",
            "Flags a #[cfg(...)] predicate copied across multiple item kinds in one file, or repeated many times on one kind. #[cfg] on a mod is never scanned. Field- and variant-only gating is never flagged. Thresholds: [cfg_scatter] min_distinct_kinds / min_occurrences.",
            "`[cfg_scatter] enabled = false` in cordial.toml.",
            &[EtiquetteRuleExplain::new(
                "CFG-SCATTER-001",
                "Scattered #[cfg] that belongs on a mod",
            )],
        ),
    ),
    Some(QualityAreaSpec::new(
        "Cfg scatter",
        "cfg-scatter.checklist.md",
        "cfg-scatter-summary.md",
        quality_area_compute,
    )),
);

#[instrument(level = "debug", skip(findings))]
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let cfg_scatter = count_open_category(findings, "cfg_scatter");
    (
        cfg_scatter,
        format!("scattered `#[cfg]` groups **{cfg_scatter}**"),
    )
}
