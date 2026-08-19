//! Inventory of `#[allow]` / `#![allow]` attributes.
//!
//! **What.** Records every `#[allow(...)]` and inner `#![allow(...)]`
//! (`ALLOW-ATTR-001`). It does not decide whether the lint is justified.
//!
//! **Why.** Allows hide compiler and Clippy signal. A regeneratable catalog
//! makes each suppression reviewable, exceptionable, and comparable across
//! crates instead of disappearing into the source.
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
pub use types::AllowRuleId;

use crate::etiquette::StaticEtiquette;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

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
pub static ALLOWS_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "allows",
    name: "Allow attributes",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: None,
    reporters: REPORTERS,
    is_coverage: false,
};
