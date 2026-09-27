//! Glob `use` trees (`foo::*`).
//!
//! **What.** Finds imports that hide the concrete names a file depends on.
//!
//! **Why.** Glob imports break reviewability and completion when code moves.
//! Explicit lists keep dependencies, tracing recipes, and exception patches
//! pointed at real identifiers.
//!
//! **Flags.** Every `*` in a `use` item as `GLOB-IMPORT-001`, including
//! `pub use`, `use super::*`, and nested `use foo::{bar, *}`.
//!
//! **Ignores.** `use <path>::prelude::*` is allowed because prelude modules
//! are conventionally designed for glob import.
//!
//! **Outputs.** `{store}/findings/glob-imports.checklist.md`,
//! `glob-imports-summary.md`, and CSV.
//!
//! **Config.** `[glob_imports] enabled = false` opts out in `cordial.toml`.
//! Exceptions are managed with `cordial exceptions show glob_imports`.
//! Register [`GLOB_IMPORTS_ETIQUETTE`]. Policy:
//! `docs/planning/glob-imports-etiquette.md`.

mod assessor;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::GlobImportAssessor;
pub use enricher::GlobImportInventoryEnricher;
pub use probe::GlobImportSiteProbe;
pub use reporter::{GlobImportChecklistReporter, GlobImportCsvReporter, GlobImportSummaryReporter};
pub use scan::{scan_crate_glob_imports, scan_rust_source};
pub use types::GlobImportRuleId;

use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
    StaticQualityEtiquette, count_open_category,
};
use crate::objects::Finding;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

use tracing::instrument;

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static GLOB_IMPORT_INVENTORY: GlobImportInventoryEnricher = GlobImportInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static GLOB_IMPORT_PROBE: GlobImportSiteProbe = GlobImportSiteProbe;
static GLOB_IMPORT_ASSESSOR: GlobImportAssessor = GlobImportAssessor;
static GLOB_IMPORT_CSV: GlobImportCsvReporter = GlobImportCsvReporter;
static GLOB_IMPORT_CHECKLIST: GlobImportChecklistReporter = GlobImportChecklistReporter;
static GLOB_IMPORT_SUMMARY: GlobImportSummaryReporter = GlobImportSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] =
    &[&SCOPE_ENRICHER, &GLOB_IMPORT_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&'static dyn crate::Probe] = &[&GLOB_IMPORT_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&GLOB_IMPORT_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &GLOB_IMPORT_CSV,
    &GLOB_IMPORT_CHECKLIST,
    &GLOB_IMPORT_SUMMARY,
];

/// Built-in glob-imports etiquette bundle.
pub static GLOB_IMPORTS_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette::new(
    StaticEtiquette::new(
        "glob_imports",
        "Glob imports",
        EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
        false,
        EtiquetteExplain::new(
            "Are there glob use trees (foo::*)?",
            "Glob imports hide which names a file depends on and break completion. Explicit lists stay reviewable when code moves.",
            "Flags every * in a use item, including pub use, use super::*, and nested use foo::{bar, *}.",
            "`[glob_imports] enabled = false` in cordial.toml.",
            &[EtiquetteRuleExplain::new(
                "GLOB-IMPORT-001",
                "A glob `use` tree",
            )],
        ),
    ),
    Some(QualityAreaSpec::new(
        "Glob imports",
        "glob-imports.checklist.md",
        "glob-imports-summary.md",
        quality_area_compute,
    )),
);

#[instrument(level = "debug", skip(findings))]
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let glob_imports = count_open_category(findings, "glob_imports");
    (glob_imports, format!("glob `use` sites **{glob_imports}**"))
}
