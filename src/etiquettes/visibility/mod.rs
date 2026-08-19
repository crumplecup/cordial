//! Public module paths must earn their existence.
//!
//! **What.** Three rules ([`VisibilityRuleId`]): a small crate stays flat
//! (`VIS-CRATE-FLAT-001`); a visible module needs enough leaf names
//! (`VIS-MOD-THIN-001`); a child’s visibility must not exceed its parent
//! (`VIS-MOD-MISMATCH-001`). Pub *fields* stay in `derives`.
//!
//! **Why.** `pub mod` is a promise of a public path. A thin module or a
//! `pub` child of a private parent splits crate-internal navigation without
//! buying a real API. Companion to `modularity` (mass) and `cfg_scatter`
//! (gates).
//!
//! **How to use.** Run `cordial quality` (feature `visibility`). Thresholds
//! live under `[visibility]` in `cordial.toml`. When flattening would overflow
//! the crate-name cap, `prefer_root` (default true) keeps a fat root.
//! Artifacts: `{store}/findings/visibility.checklist.md` and
//! `visibility-summary.md`. Register [`VISIBILITY_ETIQUETTE`].
//!
//! Policy: `docs/planning/visibility-etiquette.md`.

mod assessor;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::VisibilityAssessor;
pub use enricher::VisibilityInventoryEnricher;
pub use probe::VisibilitySiteProbe;
pub use reporter::{VisibilityChecklistReporter, VisibilityCsvReporter, VisibilitySummaryReporter};
pub use scan::{BranchingCache, scan_crate_visibility, scan_crate_visibility_with_cache};
pub use types::{
    VisibilityRecord, VisibilityRuleId, VisibilityThresholds, load_visibility_thresholds,
};

use crate::etiquette::StaticEtiquette;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static VISIBILITY_INVENTORY: VisibilityInventoryEnricher = VisibilityInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static VISIBILITY_PROBE: VisibilitySiteProbe = VisibilitySiteProbe;
static VISIBILITY_ASSESSOR: VisibilityAssessor = VisibilityAssessor;
static VISIBILITY_CSV: VisibilityCsvReporter = VisibilityCsvReporter;
static VISIBILITY_CHECKLIST: VisibilityChecklistReporter = VisibilityChecklistReporter;
static VISIBILITY_SUMMARY: VisibilitySummaryReporter = VisibilitySummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] =
    &[&SCOPE_ENRICHER, &VISIBILITY_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&'static dyn crate::Probe] = &[&VISIBILITY_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&VISIBILITY_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] =
    &[&VISIBILITY_CSV, &VISIBILITY_CHECKLIST, &VISIBILITY_SUMMARY];

/// Built-in visibility etiquette: `pub mod` paths must earn their existence.
pub static VISIBILITY_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "visibility",
    name: "Module visibility",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: None,
    reporters: REPORTERS,
    is_coverage: false,
};
