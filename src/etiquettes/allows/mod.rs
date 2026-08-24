//! Inventory of `#[allow]` / `#![allow]` attributes.
//!
//! **What.** Records every `#[allow(...)]` and inner `#![allow(...)]`
//! (`ALLOW-ATTR-001`). Verus is the one judged case: an allow on a
//! `vstd` / `verus_builtin` import must carry rustc's `reason = "..."`
//! (`ALLOW-VERUS-REASON-001`). A reasoned Verus allow is not an action
//! item — the prelude is unused under plain rustc because `verus! {}`
//! erases spec content.
//!
//! **Why.** Allows hide compiler and Clippy signal. A regeneratable catalog
//! makes each suppression reviewable, exceptionable, and comparable across
//! crates instead of disappearing into the source. Verus globs are the
//! accepted exception; they still have to say why.
//!
//! **How to use.** Run `cordial quality` (feature `allows`, part of
//! `quality`). Artifacts: `{store}/findings/allows.checklist.md`,
//! `allows-summary.md`, and CSV. Exceptions: `cordial exceptions show allows`.
//! Register [`ALLOWS_ETIQUETTE`] on a [`crate::Session`].

mod assessor;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::AllowAssessor;
pub use enricher::AllowInventoryEnricher;
pub use probe::AllowSiteProbe;
pub use reporter::{AllowChecklistReporter, AllowCsvReporter, AllowSummaryReporter};
pub use scan::{scan_crate_allows, scan_rust_source};
pub use types::{AllowRuleId, AllowSiteRecord};

use crate::etiquette::{
    QualityAreaSpec, StaticEtiquette, StaticQualityEtiquette, count_open_category,
};
use crate::objects::Finding;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

use tracing::instrument;

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static ALLOW_INVENTORY: AllowInventoryEnricher = AllowInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static ALLOW_PROBE: AllowSiteProbe = AllowSiteProbe;
static ALLOW_ASSESSOR: AllowAssessor = AllowAssessor;
static ALLOW_CSV: AllowCsvReporter = AllowCsvReporter;
static ALLOW_CHECKLIST: AllowChecklistReporter = AllowChecklistReporter;
static ALLOW_SUMMARY: AllowSummaryReporter = AllowSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] =
    &[&SCOPE_ENRICHER, &ALLOW_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&'static dyn crate::Probe] = &[&ALLOW_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&ALLOW_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[&ALLOW_CSV, &ALLOW_CHECKLIST, &ALLOW_SUMMARY];

/// Built-in allows etiquette bundle.
pub static ALLOWS_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette {
    etiquette: StaticEtiquette {
        id: "allows",
        name: "Allow attributes",
        loaders: LOADERS,
        enrichers: ENRICHERS,
        probes: PROBES,
        assessors: ASSESSORS,
        workspace_assessors: None,
        reporters: REPORTERS,
        is_coverage: false,
    },
    quality_area: Some(QualityAreaSpec {
        title: "Allow attributes",
        checklist: "allows.checklist.md",
        summary: "allows-summary.md",
        compute: quality_area_compute,
    }),
};

#[instrument(level = "debug", skip(findings))]
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let allows = count_open_category(findings, "allows");
    (allows, format!("allow attributes **{allows}**"))
}
