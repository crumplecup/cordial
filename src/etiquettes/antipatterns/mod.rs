//! Untyped error carriers and related source smells.
//!
//! **What.** Flags `Box<dyn Error>`, `Result<_, String>`, unused `_arg`
//! parameters (except on impls of traits not defined in this crate), struct
//! fields that are `&'static` references where an owned type would do
//! (`Location` → copy `file`/`line`; `&'static dyn` of a crate-local trait is
//! a view/registry exception; `&'static str` on types constructed only as
//! `const`/`static` is a placement guarantee), unnamed contract bounds (Kani / Creusot /
//! Verus), and workspace members that pin a version in
//! `[workspace.members]`-adjacent tables ([`AntipatternRuleId`]).
//!
//! **Why.** These are quality problems adjacent to error handling that are
//! not site/chain/foreign layers: they erase types, hide unused work, or
//! fight workspace versioning. The error-handling plugin consumes typed
//! `E`; this etiquette catches the untyped leftovers.
//!
//! **How to use.** Run `cordial quality` (feature `antipatterns`). Artifacts:
//! `{store}/findings/antipatterns.checklist.md`, `antipatterns-summary.md`,
//! plus `version-in-member.*`. Register [`ANTIPATTERNS_ETIQUETTE`].

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

use crate::etiquette::{
    EtiquetteExplain, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
    StaticQualityEtiquette, count_open_rule,
};
use crate::objects::Finding;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

use tracing::instrument;

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
pub static ANTIPATTERNS_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette {
    etiquette: StaticEtiquette {
        id: "antipatterns",
        name: "Antipatterns",
        loaders: LOADERS,
        enrichers: ENRICHERS,
        probes: PROBES,
        assessors: ASSESSORS,
        workspace_assessors: None,
        reporters: REPORTERS,
        is_coverage: false,
        explain: EtiquetteExplain {
            summary: "Untyped error carriers and related source smells?",
            why: "These are quality problems adjacent to error handling that are not site/chain/foreign layers: they erase types, hide unused work, or fight workspace versioning.",
            logic: "Flags Box<dyn Error>, Result<_, String>, unused _arg (except on impls of foreign traits), struct &'static fields where an owned type would do, unnamed contract bounds (Kani/Creusot/Verus), and workspace members that pin a version. Some Box<dyn Error> / unused-arg rows feed the Error handling quality-report area.",
            opt_out: "`[antipatterns] enabled = false` in cordial.toml.",
            rules: &[
                EtiquetteRuleExplain {
                    id: "ANTIPATTERN-BOX-DYN-ERROR-001",
                    summary: "`Box<dyn Error>` carrier",
                },
                EtiquetteRuleExplain {
                    id: "ANTIPATTERN-STRING-ERROR-001",
                    summary: "`Result<_, String>` carrier",
                },
                EtiquetteRuleExplain {
                    id: "ANTIPATTERN-UNUSED-UNDERSCORE-ARG-001",
                    summary: "Unused `_arg` parameter",
                },
                EtiquetteRuleExplain {
                    id: "ANTIPATTERN-STRUCT-STATIC-REF-001",
                    summary: "`&'static` field that should be owned",
                },
                EtiquetteRuleExplain {
                    id: "ANTIPATTERN-UNNAMED-CONTRACT-BOUND-001",
                    summary: "Unnamed verifier contract bound",
                },
                EtiquetteRuleExplain {
                    id: "ANTIPATTERN-VERSION-IN-MEMBER-001",
                    summary: "Version pin on a workspace member",
                },
            ],
        },
    },
    quality_area: Some(QualityAreaSpec {
        title: "Antipatterns",
        checklist: "antipatterns.checklist.md",
        summary: "antipatterns-summary.md",
        compute: quality_area_compute,
    }),
};

/// `Box<dyn Error>`/`Result<_, String>` (`ANTIPATTERN-BOX-DYN-ERROR-001`/
/// `ANTIPATTERN-STRING-ERROR-001`) are deliberately excluded here -- they
/// feed the hand-composed "Error handling" area instead (see
/// `reporter::quality_report`), since they're specifically about untyped
/// error carriers, not this etiquette's other, unrelated smells.
#[instrument(level = "debug", skip(findings))]
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let unused_arg = count_open_rule(findings, "ANTIPATTERN-UNUSED-UNDERSCORE-ARG-001");
    let static_ref = count_open_rule(findings, "ANTIPATTERN-STRUCT-STATIC-REF-001");
    let version_in_member = count_open_rule(findings, "ANTIPATTERN-VERSION-IN-MEMBER-001");
    let unnamed_contract = count_open_rule(findings, "ANTIPATTERN-UNNAMED-CONTRACT-BOUND-001");
    let total = unused_arg + static_ref + version_in_member + unnamed_contract;
    let detail = format!(
        "unused `_arg` **{unused_arg}**, static refs **{static_ref}**, \
         version-in-member **{version_in_member}**, unnamed contract **{unnamed_contract}**"
    );
    (total, detail)
}
