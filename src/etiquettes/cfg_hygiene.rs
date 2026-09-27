//! Undeclared `cfg` names and verifier cfg names in the wrong backend crate.
//!
//! **What.** Checks cfg names against the declarations reachable by a crate
//! and against any configured verifier identity.
//!
//! **Why.** A workspace-wide `--check-cfg` union can make a copied
//! `#[cfg(creusot)]` in a Kani-only crate invisible to rustc. Project-aware
//! cfg hygiene catches that backend mismatch and the general case of names
//! neither rustc nor the project declared.
//!
//! **Flags.** `cfg(X)` / `cfg_attr(X, ...)`, including nested
//! `all()` / `any()` / `not()`, whose name is undeclared
//! (`UNEXPECTED-CFG-001`), and crates using a different verifier cfg than the
//! one configured in `[cfg_hygiene] crate_verifier`
//! (`CFG-VERIFIER-MISMATCH-001`).
//!
//! **Ignores.** Rustc built-ins and Cargo-known names are treated as known.
//! Verifier mismatch is inert until the project fills `crate_verifier`.
//!
//! **Outputs.** `{store}/findings/cfg-hygiene.checklist.md`,
//! `cfg-hygiene-summary.md`, and CSV.
//!
//! **Config.** `[cfg_hygiene]` owns `crate_verifier`, `extra_known_names`,
//! and `enabled`. Register [`CFG_HYGIENE_ETIQUETTE`]. Policy:
//! `docs/planning/cfg-hygiene-etiquette.md`.

mod assessor;
mod declared;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod scan_crate;
mod types;

pub use assessor::CfgHygieneAssessor;
pub use declared::{all_verifier_names, declared_names_for_crate, expected_verifier_for};
pub use enricher::CfgHygieneInventoryEnricher;
pub use probe::CfgHygieneSiteProbe;
pub use reporter::{CfgHygieneChecklistReporter, CfgHygieneCsvReporter, CfgHygieneSummaryReporter};
pub use scan::scan_rust_source;
pub use scan_crate::scan_crate_cfg_hygiene;
pub use types::{CfgHygieneRuleId, CfgHygieneSiteRecord};

use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
    StaticQualityEtiquette, count_open_rule,
};
use crate::objects::Finding;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

use tracing::instrument;

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static CFG_HYGIENE_INVENTORY: CfgHygieneInventoryEnricher = CfgHygieneInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static CFG_HYGIENE_PROBE: CfgHygieneSiteProbe = CfgHygieneSiteProbe;
static CFG_HYGIENE_ASSESSOR: CfgHygieneAssessor = CfgHygieneAssessor;
static CFG_HYGIENE_CSV: CfgHygieneCsvReporter = CfgHygieneCsvReporter;
static CFG_HYGIENE_CHECKLIST: CfgHygieneChecklistReporter = CfgHygieneChecklistReporter;
static CFG_HYGIENE_SUMMARY: CfgHygieneSummaryReporter = CfgHygieneSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] =
    &[&SCOPE_ENRICHER, &CFG_HYGIENE_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&'static dyn crate::Probe] = &[&CFG_HYGIENE_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&CFG_HYGIENE_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &CFG_HYGIENE_CSV,
    &CFG_HYGIENE_CHECKLIST,
    &CFG_HYGIENE_SUMMARY,
];

/// Built-in cfg-hygiene etiquette bundle: undeclared cfg names, and
/// verifier cfg names leaking into the wrong backend crate.
pub static CFG_HYGIENE_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette::new(
    StaticEtiquette::new(
        "cfg_hygiene",
        "Cfg hygiene",
        EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
        false,
        EtiquetteExplain::new(
            "Is every cfg name declared, and does each verifier crate only use its own?",
            "A workspace-wide --check-cfg union makes a copy-pasted #[cfg(creusot)] in a Kani-only crate invisible to rustc. Nothing short of a project-aware scan can catch it.",
            "UNEXPECTED-CFG-001: a cfg(X) / cfg_attr(X) whose X is not declared anywhere reachable by that crate. CFG-VERIFIER-MISMATCH-001: a crate in [cfg_hygiene] crate_verifier using a different verifier's cfg name than its configured identity (inert until crate_verifier is filled).",
            "`[cfg_hygiene] enabled = false` in cordial.toml.",
            &[
                EtiquetteRuleExplain::new("UNEXPECTED-CFG-001", "cfg name rustc would not expect"),
                EtiquetteRuleExplain::new(
                    "CFG-VERIFIER-MISMATCH-001",
                    "Verifier cfg used in the wrong crate",
                ),
            ],
        ),
    ),
    Some(QualityAreaSpec::new(
        "Cfg hygiene",
        "cfg-hygiene.checklist.md",
        "cfg-hygiene-summary.md",
        quality_area_compute,
    )),
);

#[instrument(level = "debug", skip(findings))]
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let unexpected = count_open_rule(findings, "UNEXPECTED-CFG-001");
    let verifier_mismatch = count_open_rule(findings, "CFG-VERIFIER-MISMATCH-001");
    let total = unexpected + verifier_mismatch;
    (
        total,
        format!(
            "undeclared cfg names **{unexpected}**, verifier mismatches **{verifier_mismatch}**"
        ),
    )
}
