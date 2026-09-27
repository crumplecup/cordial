//! Dependency freshness survey.
//!
//! **What.** Reads direct dependency declarations from `Cargo.toml` and joins
//! them to resolved package versions from `Cargo.lock`.
//!
//! **Why.** Outdated dependency policy needs manifest intent (`workspace =
//! true`, workspace-level policy, exact pins, wildcards, upper bounds, source
//! kind), lockfile resolution, and Cargo's registry freshness view. This slice
//! keeps the survey available while promoting selected manifest and registry
//! indicators into independent lints.
//!
//! **Flags.** `cargo update --dry-run --verbose` observations produce
//! `DEPENDENCY-FRESHNESS-PATCH`, `DEPENDENCY-FRESHNESS-MINOR`, and
//! `DEPENDENCY-FRESHNESS-MAJOR`. Manifest policy indicators produce exact-pin,
//! upper-bound, wildcard, tilde, and workspace-bypass rule ids. A cache file
//! under the session store can override Cargo for deterministic runs.
//!
//! **Outputs.** `{store}/findings/dependency-freshness-survey.csv`,
//! `dependency-freshness.checklist.md`, `dependency-freshness-summary.md`, and
//! CSV.
//!
//! **Config.** `[dependency_freshness] enabled = false` opts out once compiled.
//! Policy: `docs/planning/dependency-freshness-etiquette.md`.

mod assessor;
mod enricher;
mod loader;
mod probe;
mod reporter;
mod scan;
mod types;

use assessor::DependencyFreshnessAssessor;
use enricher::DependencyFreshnessSurveyEnricher;
use loader::DependencyFreshnessLoader;
use probe::DependencyFreshnessSiteProbe;
use reporter::{
    DependencyFreshnessChecklistReporter, DependencyFreshnessCsvReporter,
    DependencyFreshnessSummaryReporter, DependencyFreshnessSurveyReporter,
};

use crate::SourceLoader;
use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
    StaticQualityEtiquette, count_open_category,
};
use crate::objects::Finding;

use tracing::instrument;

static SOURCE_LOADER: SourceLoader = SourceLoader;
static DEPENDENCY_FRESHNESS_LOADER: DependencyFreshnessLoader = DependencyFreshnessLoader;
static DEPENDENCY_FRESHNESS_ENRICHER: DependencyFreshnessSurveyEnricher =
    DependencyFreshnessSurveyEnricher;
static DEPENDENCY_FRESHNESS_PROBE: DependencyFreshnessSiteProbe = DependencyFreshnessSiteProbe;
static DEPENDENCY_FRESHNESS_ASSESSOR: DependencyFreshnessAssessor = DependencyFreshnessAssessor;
static DEPENDENCY_FRESHNESS_REPORTER: DependencyFreshnessSurveyReporter =
    DependencyFreshnessSurveyReporter;
static DEPENDENCY_FRESHNESS_CSV: DependencyFreshnessCsvReporter = DependencyFreshnessCsvReporter;
static DEPENDENCY_FRESHNESS_CHECKLIST: DependencyFreshnessChecklistReporter =
    DependencyFreshnessChecklistReporter;
static DEPENDENCY_FRESHNESS_SUMMARY: DependencyFreshnessSummaryReporter =
    DependencyFreshnessSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER, &DEPENDENCY_FRESHNESS_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] = &[&DEPENDENCY_FRESHNESS_ENRICHER];
static PROBES: &[&'static dyn crate::Probe] = &[&DEPENDENCY_FRESHNESS_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&DEPENDENCY_FRESHNESS_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &DEPENDENCY_FRESHNESS_REPORTER,
    &DEPENDENCY_FRESHNESS_CSV,
    &DEPENDENCY_FRESHNESS_CHECKLIST,
    &DEPENDENCY_FRESHNESS_SUMMARY,
];

/// Built-in dependency-freshness survey bundle.
pub static DEPENDENCY_FRESHNESS_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette::new(
    StaticEtiquette::new(
        "dependency_freshness",
        "Dependency freshness",
        EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
        false,
        EtiquetteExplain::new(
            "Which dependency declarations and lockfile resolutions can freshness lints use?",
            "Dependency freshness needs manifest intent and lockfile state before registry comparisons can be judged.",
            "Surveys direct Cargo.toml dependencies, classifies manifest indicators, joins matching Cargo.lock package versions, asks Cargo for available registry updates, and emits manifest-policy plus patch/minor/major findings.",
            "`[dependency_freshness] enabled = false` in cordial.toml.",
            &[
                EtiquetteRuleExplain::new(
                    "DEPENDENCY-FRESHNESS-PATCH",
                    "A newer patch release is available",
                ),
                EtiquetteRuleExplain::new(
                    "DEPENDENCY-FRESHNESS-MINOR",
                    "A newer minor release is available",
                ),
                EtiquetteRuleExplain::new(
                    "DEPENDENCY-FRESHNESS-MAJOR",
                    "A newer major release is available",
                ),
                EtiquetteRuleExplain::new(
                    "DEPENDENCY-FRESHNESS-MANIFEST-EXACT-PIN",
                    "A dependency manifest uses an exact version pin",
                ),
                EtiquetteRuleExplain::new(
                    "DEPENDENCY-FRESHNESS-MANIFEST-UPPER-BOUND",
                    "A dependency manifest uses an upper-bound version requirement",
                ),
                EtiquetteRuleExplain::new(
                    "DEPENDENCY-FRESHNESS-MANIFEST-WILDCARD",
                    "A dependency manifest uses a wildcard version requirement",
                ),
                EtiquetteRuleExplain::new(
                    "DEPENDENCY-FRESHNESS-MANIFEST-TILDE",
                    "A dependency manifest uses a tilde version requirement",
                ),
                EtiquetteRuleExplain::new(
                    "DEPENDENCY-FRESHNESS-MANIFEST-WORKSPACE-BYPASS",
                    "A member dependency bypasses a matching workspace dependency policy",
                ),
            ],
        ),
    ),
    Some(QualityAreaSpec::new(
        "Dependency freshness",
        "dependency-freshness.checklist.md",
        "dependency-freshness-summary.md",
        quality_area_compute,
    )),
);

#[instrument(level = "debug", skip(findings))]
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let dependency_freshness = count_open_category(findings, "dependency_freshness");
    (
        dependency_freshness,
        format!("dependency freshness items **{dependency_freshness}**"),
    )
}
