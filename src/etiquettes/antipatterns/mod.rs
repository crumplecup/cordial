//! Untyped error carriers and related source smells.
//!
//! **What.** Catches quality problems adjacent to error handling that do not
//! belong to the site, chain, foreign-type, or attenuation layers.
//!
//! **Why.** These patterns erase types, hide unused work, or fight workspace
//! versioning. The error-handling plugin consumes typed `E`; this etiquette
//! catches the untyped leftovers and adjacent smells.
//!
//! **Flags.** `Box<dyn Error>`, `Result<_, String>`, unused `_arg`
//! parameters, struct fields that are `&'static` references where owned data
//! would do, unnamed verifier contract bounds, and workspace members that pin
//! a version in workspace-adjacent tables.
//!
//! **Ignores.** Unused `_arg` is allowed on impls of traits not defined in
//! this crate. `&'static dyn` of a crate-local trait may be a view/registry
//! exception, and `&'static str` on const/static-only types may be a placement
//! guarantee.
//!
//! **Outputs.** `{store}/findings/antipatterns.checklist.md`,
//! `antipatterns-summary.md`, antipattern CSVs, and `version-in-member.*`.
//!
//! **Config.** `[antipatterns] enabled = false` opts out in `cordial.toml`.
//! `[antipatterns.static_refs] strategy = "string" | "cow" | "const"`
//! chooses the remediation guidance for `&'static str` fields. Register
//! [`ANTIPATTERNS_ETIQUETTE`].

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
    ContractRecordDump, ContractRecordDumpBuilder, scan_crate_contract_bounds,
    scan_creusot_contract_bounds_source, scan_kani_contract_bounds_source,
    scan_verus_contract_bounds_source,
};
pub use enricher::AntipatternInventoryEnricher;
pub use probe::AntipatternSiteProbe;
pub use reporter::{
    AntipatternChecklistReporter, AntipatternCsvReporter, AntipatternSummaryReporter,
};
pub use scan::{scan_rust_source, scan_rust_source_with_static_ref_strategy};
pub use scan_crate::{scan_crate_antipatterns, scan_crate_antipatterns_with_static_ref_strategy};
pub use types::{AntipatternRuleId, AntipatternSiteRecord};
pub use version_reporter::{
    VersionInMemberChecklistReporter, VersionInMemberCsvReporter, VersionInMemberSummaryReporter,
};

use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
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
pub static ANTIPATTERNS_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette::new(
    StaticEtiquette::new(
        "antipatterns",
        "Antipatterns",
        EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
        false,
        EtiquetteExplain::new(
            "Untyped error carriers and related source smells?",
            "These are quality problems adjacent to error handling that are not site/chain/foreign layers: they erase types, hide unused work, or fight workspace versioning.",
            "Flags Box<dyn Error>, Result<_, String>, unused _arg (except on impls of foreign traits), struct &'static fields where an owned type or configured static-ref strategy would do, unnamed contract bounds (Kani/Creusot/Verus), and workspace members that pin a version. Some Box<dyn Error> / unused-arg rows feed the Error handling quality-report area.",
            "`[antipatterns] enabled = false` in cordial.toml. `[antipatterns.static_refs] strategy = \"string\" | \"cow\" | \"const\"` changes the static-ref remediation guidance; `const` falls back to `Cow<'static, str>` for runtime string fields.",
            &[
                EtiquetteRuleExplain::new(
                    "ANTIPATTERN-BOX-DYN-ERROR-001",
                    "`Box<dyn Error>` carrier",
                ),
                EtiquetteRuleExplain::new(
                    "ANTIPATTERN-STRING-ERROR-001",
                    "`Result<_, String>` carrier",
                ),
                EtiquetteRuleExplain::new(
                    "ANTIPATTERN-UNUSED-UNDERSCORE-ARG-001",
                    "Unused `_arg` parameter",
                ),
                EtiquetteRuleExplain::new(
                    "ANTIPATTERN-STRUCT-STATIC-REF-001",
                    "`&'static` field that should be owned",
                ),
                EtiquetteRuleExplain::new(
                    "ANTIPATTERN-UNNAMED-CONTRACT-BOUND-001",
                    "Unnamed verifier contract bound",
                ),
                EtiquetteRuleExplain::new(
                    "ANTIPATTERN-VERSION-IN-MEMBER-001",
                    "Version pin on a workspace member",
                ),
            ],
        ),
    ),
    Some(QualityAreaSpec::new(
        "Antipatterns",
        "antipatterns.checklist.md",
        "antipatterns-summary.md",
        quality_area_compute,
    )),
);

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
