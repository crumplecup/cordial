//! Glob `use` trees (`foo::*`).
//!
//! **What.** Flags every `*` in a `use` item (`GLOB-IMPORT-001`), including
//! `pub use`, `use super::*;`, and nested `use foo::{bar, *}`. Exempts
//! `use <path>::prelude::*;` -- a crate's own `prelude` module is
//! conventionally designed to be glob-imported (`std::prelude`,
//! `vstd::prelude`, `itertools::prelude`, `rayon::prelude`, `diesel::
//! prelude`, ...), the same way `std`'s own prelude is auto-imported into
//! every ordinary Rust crate with no explicit-list alternative.
//!
//! **Why.** Glob imports hide which names a file depends on and break
//! completion in most IDEs. Explicit lists stay reviewable when code moves,
//! and they keep tracing recipes and exception patches pointed at real idents.
//!
//! **How to use.** Run `cordial quality` (feature `glob_imports`, part of
//! `quality`). Artifacts: `{store}/findings/glob-imports.checklist.md`,
//! `glob-imports-summary.md`, and CSV. Exceptions: `cordial exceptions show glob_imports`.
//! Register [`GLOB_IMPORTS_ETIQUETTE`] on a [`crate::Session`].
//!
//! Policy: `docs/planning/glob-imports-etiquette.md`.

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
    EtiquetteExplain, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
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
pub static GLOB_IMPORTS_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette {
    etiquette: StaticEtiquette {
        id: "glob_imports",
        name: "Glob imports",
        loaders: LOADERS,
        enrichers: ENRICHERS,
        probes: PROBES,
        assessors: ASSESSORS,
        workspace_assessors: None,
        reporters: REPORTERS,
        is_coverage: false,
        explain: EtiquetteExplain {
            summary: "Are there glob use trees (foo::*)?",
            why: "Glob imports hide which names a file depends on and break completion. Explicit lists stay reviewable when code moves.",
            logic: "Flags every * in a use item, including pub use, use super::*, and nested use foo::{bar, *}.",
            opt_out: "`[glob_imports] enabled = false` in cordial.toml.",
            rules: &[EtiquetteRuleExplain {
                id: "GLOB-IMPORT-001",
                summary: "A glob `use` tree",
            }],
        },
    },
    quality_area: Some(QualityAreaSpec {
        title: "Glob imports",
        checklist: "glob-imports.checklist.md",
        summary: "glob-imports-summary.md",
        compute: quality_area_compute,
    }),
};

#[instrument(level = "debug", skip(findings))]
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let glob_imports = count_open_category(findings, "glob_imports");
    (glob_imports, format!("glob `use` sites **{glob_imports}**"))
}
