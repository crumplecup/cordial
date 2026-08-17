mod assessor;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod syntax;
mod types;

pub use assessor::DeriveAssessor;
pub use enricher::DeriveInventoryEnricher;
pub use probe::DeriveSiteProbe;
pub use reporter::{DeriveChecklistReporter, DeriveCsvReporter, DeriveSummaryReporter};
pub use scan::scan_rust_source;
pub use types::DeriveRuleId;

use crate::etiquette::StaticEtiquette;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static DERIVE_INVENTORY: DeriveInventoryEnricher = DeriveInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static DERIVE_PROBE: DeriveSiteProbe = DeriveSiteProbe;
static DERIVE_ASSESSOR: DeriveAssessor = DeriveAssessor;
static DERIVE_CSV: DeriveCsvReporter = DeriveCsvReporter;
static DERIVE_CHECKLIST: DeriveChecklistReporter = DeriveChecklistReporter;
static DERIVE_SUMMARY: DeriveSummaryReporter = DeriveSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] =
    &[&SCOPE_ENRICHER, &DERIVE_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&'static dyn crate::Probe] = &[&DERIVE_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&DERIVE_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] =
    &[&DERIVE_CSV, &DERIVE_CHECKLIST, &DERIVE_SUMMARY];

/// Built-in derives etiquette bundle.
pub static DERIVES_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "derives",
    name: "Derive patterns",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: None,
    reporters: REPORTERS,
    is_coverage: false,
};
