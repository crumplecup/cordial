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
use crate::etiquette::StaticEtiquette;

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
pub static CLI_LAYOUT_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "cli_layout",
    name: "CLI layout",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: None,
    reporters: REPORTERS,
    is_coverage: false,
};
