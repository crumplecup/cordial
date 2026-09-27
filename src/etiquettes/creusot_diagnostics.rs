//! Diagnostics from `cargo creusot prove`.
//!
//! **What.** Runs `cargo creusot prove` on Creusot compilation units and
//! captures diagnostics ordinary Rust tooling cannot see.
//!
//! **Why.** Creusot proof obligations run outside `cargo check` and Clippy.
//! A clean Rust build can still hide proof warnings or verifier failures.
//!
//! **Flags.** Each `warning:` diagnostic as `CREUSOT-DIAGNOSTIC-001`; each
//! `error:` diagnostic or failed prove run as `CREUSOT-DIAGNOSTIC-002`.
//! Summary lines are dropped and duplicate spans are kept once.
//!
//! **Ignores.** Crates that are not Creusot targets are skipped. The scan also
//! skips when no Creusot runner is available.
//!
//! **Outputs.** `{store}/findings/creusot-diagnostics.checklist.md`,
//! `creusot-diagnostics-summary.md`, and CSV.
//!
//! **Config.** `[creusot_diagnostics] enabled = false` opts out in `cordial.toml`.
//! `[creusot_diagnostics] skip_crates = ["..."]` skips selected packages.
//! `CORDIAL_CREUSOT` overrides the Creusot runner.
//! Exceptions are managed with `cordial exceptions show creusot_diagnostics`.
//! Register [`CREUSOT_DIAGNOSTICS_ETIQUETTE`]. Policy:
//! `docs/planning/creusot-diagnostics-etiquette.md`.

mod assessor;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::CreusotDiagnosticAssessor;
pub use enricher::CreusotDiagnosticInventoryEnricher;
pub use probe::CreusotDiagnosticSiteProbe;
pub use reporter::{
    CreusotDiagnosticChecklistReporter, CreusotDiagnosticCsvReporter,
    CreusotDiagnosticSummaryReporter,
};
pub use scan::{
    crate_is_creusot_target, parse_creusot_compiler_output, scan_crate_creusot_diagnostics,
};
pub use types::{CreusotDiagnosticRecord, CreusotDiagnosticRuleId};

use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
    StaticQualityEtiquette, count_open_category,
};
use crate::objects::Finding;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

use tracing::instrument;

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static CREUSOT_DIAGNOSTIC_INVENTORY: CreusotDiagnosticInventoryEnricher =
    CreusotDiagnosticInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static CREUSOT_DIAGNOSTIC_PROBE: CreusotDiagnosticSiteProbe = CreusotDiagnosticSiteProbe;
static CREUSOT_DIAGNOSTIC_ASSESSOR: CreusotDiagnosticAssessor = CreusotDiagnosticAssessor;
static CREUSOT_DIAGNOSTIC_CSV: CreusotDiagnosticCsvReporter = CreusotDiagnosticCsvReporter;
static CREUSOT_DIAGNOSTIC_CHECKLIST: CreusotDiagnosticChecklistReporter =
    CreusotDiagnosticChecklistReporter;
static CREUSOT_DIAGNOSTIC_SUMMARY: CreusotDiagnosticSummaryReporter =
    CreusotDiagnosticSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] = &[
    &SCOPE_ENRICHER,
    &CREUSOT_DIAGNOSTIC_INVENTORY,
    &ATTRIBUTE_ENRICHER,
];
static PROBES: &[&'static dyn crate::Probe] = &[&CREUSOT_DIAGNOSTIC_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&CREUSOT_DIAGNOSTIC_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &CREUSOT_DIAGNOSTIC_CSV,
    &CREUSOT_DIAGNOSTIC_CHECKLIST,
    &CREUSOT_DIAGNOSTIC_SUMMARY,
];

/// Built-in Creusot diagnostic etiquette bundle.
pub static CREUSOT_DIAGNOSTICS_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette::new(
    StaticEtiquette::new(
        "creusot_diagnostics",
        "Creusot diagnostics",
        EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
        false,
        EtiquetteExplain::new(
            "Does `cargo creusot prove` emit warnings or verification failures?",
            "Creusot proof obligations run outside cargo check and clippy, so their diagnostics need their own feedback loop.",
            "Invokes `cargo creusot prove -- -p <crate>` for crates ending `_creusot` or depending on creusot-std / creusot_contracts. `CORDIAL_CREUSOT` can supply a custom runner. Records each warning/error diagnostic; nonzero output with no parseable error becomes a crate-level failure. Skips when no cargo-creusot/CORDIAL_CREUSOT runner exists or `[creusot_diagnostics] skip_crates` names the package.",
            "`[creusot_diagnostics] enabled = false` in cordial.toml.",
            &[
                EtiquetteRuleExplain::new(
                    "CREUSOT-DIAGNOSTIC-001",
                    "A `warning:` diagnostic from `cargo creusot prove`",
                ),
                EtiquetteRuleExplain::new(
                    "CREUSOT-DIAGNOSTIC-002",
                    "An `error:` diagnostic or failed `cargo creusot prove` run",
                ),
            ],
        ),
    ),
    Some(QualityAreaSpec::new(
        "Creusot diagnostics",
        "creusot-diagnostics.checklist.md",
        "creusot-diagnostics-summary.md",
        quality_area_compute,
    )),
);

#[instrument(level = "debug", skip(findings))]
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let creusot_diagnostics = count_open_category(findings, "creusot_diagnostics");
    (
        creusot_diagnostics,
        format!("Creusot diagnostics **{creusot_diagnostics}**"),
    )
}
