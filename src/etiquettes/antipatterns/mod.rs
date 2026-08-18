mod assessor;
mod contract_bounds;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod scan_crate;
mod types;
mod version_in_member;
mod version_reporter;

pub use assessor::AntipatternAssessor;
pub use contract_bounds::{
    ContractRecordDump, scan_crate_contract_bounds, scan_creusot_contract_bounds_source,
    scan_kani_contract_bounds_source, scan_verus_contract_bounds_source,
};
pub use enricher::AntipatternInventoryEnricher;
pub use probe::AntipatternSiteProbe;
pub use reporter::{
    AntipatternChecklistReporter, AntipatternCsvReporter, AntipatternSummaryReporter,
};
pub use scan::scan_rust_source;
pub use scan_crate::scan_crate_antipatterns;
pub use types::{AntipatternRuleId, AntipatternSiteRecord};
pub use version_reporter::{
    VersionInMemberChecklistReporter, VersionInMemberCsvReporter, VersionInMemberSummaryReporter,
};

use crate::etiquette::StaticEtiquette;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static ANTIPATTERN_INVENTORY: AntipatternInventoryEnricher = AntipatternInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static ANTIPATTERN_PROBE: AntipatternSiteProbe = AntipatternSiteProbe;
static ANTIPATTERN_ASSESSOR: AntipatternAssessor = AntipatternAssessor;
static ANTIPATTERN_CSV: AntipatternCsvReporter = AntipatternCsvReporter;
static ANTIPATTERN_CHECKLIST: AntipatternChecklistReporter = AntipatternChecklistReporter;
static ANTIPATTERN_SUMMARY: AntipatternSummaryReporter = AntipatternSummaryReporter;
static VERSION_IN_MEMBER_CSV: VersionInMemberCsvReporter = VersionInMemberCsvReporter;
static VERSION_IN_MEMBER_CHECKLIST: VersionInMemberChecklistReporter =
    VersionInMemberChecklistReporter;
static VERSION_IN_MEMBER_SUMMARY: VersionInMemberSummaryReporter = VersionInMemberSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] =
    &[&SCOPE_ENRICHER, &ANTIPATTERN_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&'static dyn crate::Probe] = &[&ANTIPATTERN_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&ANTIPATTERN_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &ANTIPATTERN_CSV,
    &ANTIPATTERN_CHECKLIST,
    &ANTIPATTERN_SUMMARY,
    &VERSION_IN_MEMBER_CSV,
    &VERSION_IN_MEMBER_CHECKLIST,
    &VERSION_IN_MEMBER_SUMMARY,
];

/// Built-in antipatterns etiquette bundle.
pub static ANTIPATTERNS_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "antipatterns",
    name: "Antipatterns",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: None,
    reporters: REPORTERS,
    is_coverage: false,
};
