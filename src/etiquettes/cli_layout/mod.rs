//! Clap types and dispatch live in the library; `main` is thin.
//!
//! **What.** For lib+bin crates that use clap, `Parser` / `Subcommand` types
//! must live in the library, each implement `fn act(self, …) -> Result`, and
//! hand off to every nested clap type. Free functions do not take clap types.
//! `main` only parses, calls `act`, and converts with miette. Error types
//! must not live only on the binary side.
//!
//! **Why.** A single `Cli::act` still hides a god-match and `execute_*(&Cli)`
//! helpers. Dispatch belongs on the clap types themselves.
//!
//! **How to use.** Run `cordial quality` (feature `cli_layout`). Artifacts:
//! `{store}/findings/cli-layout.checklist.md`, `cli-layout-summary.md`,
//! `cli-layout.csv`. Register [`CLI_LAYOUT_ETIQUETTE`].
//!
//! Policy: `docs/planning/one-crate-cli-layout.md`.

mod assessor;
mod enricher;
mod hunt;
mod probe;
mod reporter;
mod scan;
mod tree;
mod types;

pub use assessor::CliLayoutAssessor;
pub use enricher::CliLayoutInventoryEnricher;
pub use probe::CliLayoutSiteProbe;
pub use reporter::{CliLayoutChecklistReporter, CliLayoutCsvReporter, CliLayoutSummaryReporter};
pub use scan::scan_crate_cli_layout;
pub use types::{CliLayoutId, CliLayoutRecord};

use crate::SourceLoader;
use crate::enricher::{AttributeEnricher, ScopeEnricher};
use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
    StaticQualityEtiquette, count_open_rule,
};
use crate::objects::Finding;

use tracing::instrument;

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static CLI_LAYOUT_INVENTORY: CliLayoutInventoryEnricher = CliLayoutInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static CLI_LAYOUT_PROBE: CliLayoutSiteProbe = CliLayoutSiteProbe;
static CLI_LAYOUT_ASSESSOR: CliLayoutAssessor = CliLayoutAssessor;
static CLI_LAYOUT_CSV: CliLayoutCsvReporter = CliLayoutCsvReporter;
static CLI_LAYOUT_CHECKLIST: CliLayoutChecklistReporter = CliLayoutChecklistReporter;
static CLI_LAYOUT_SUMMARY: CliLayoutSummaryReporter = CliLayoutSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] =
    &[&SCOPE_ENRICHER, &CLI_LAYOUT_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&'static dyn crate::Probe] = &[&CLI_LAYOUT_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&CLI_LAYOUT_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] =
    &[&CLI_LAYOUT_CSV, &CLI_LAYOUT_CHECKLIST, &CLI_LAYOUT_SUMMARY];

/// Built-in CLI-layout etiquette: clap types dispatch in the library; `main` is thin.
pub static CLI_LAYOUT_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette::new(
    StaticEtiquette::new(
        "cli_layout",
        "CLI layout",
        EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
        false,
        EtiquetteExplain::new(
            "Do clap types live in the library and dispatch with act?",
            "A single Cli::act still hides a god-match and execute_*(&Cli) helpers. Dispatch belongs on the clap types themselves.",
            "For lib+bin crates that use clap, Parser / Subcommand types must live in the library, each implement fn act(self, …) -> Result, and hand off to every nested clap type. Free functions do not take clap types. main only parses, calls act, and converts with miette. Error types must not live only on the binary side.",
            "`[cli_layout] enabled = false` in cordial.toml.",
            &[
                EtiquetteRuleExplain::new("CLI-ISLAND-001", "Clap types live only on the binary"),
                EtiquetteRuleExplain::new("CLI-ACT-001", "Clap type does not dispatch with act"),
                EtiquetteRuleExplain::new(
                    "CLI-MAIN-001",
                    "main does more than parse + act + miette",
                ),
            ],
        ),
    ),
    Some(QualityAreaSpec::new(
        "CLI layout",
        "cli-layout.checklist.md",
        "cli-layout-summary.md",
        quality_area_compute,
    )),
);

#[instrument(level = "debug", skip(findings))]
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let cli_island = count_open_rule(findings, "CLI-ISLAND-001");
    let cli_act = count_open_rule(findings, "CLI-ACT-001");
    let cli_main = count_open_rule(findings, "CLI-MAIN-001");
    let total = cli_island + cli_act + cli_main;
    let detail = format!("island **{cli_island}**, act **{cli_act}**, main **{cli_main}**");
    (total, detail)
}
