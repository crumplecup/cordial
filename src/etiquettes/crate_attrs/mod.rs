//! Crate-root `#![forbid(unsafe_code)]` and `#![warn(missing_docs)]`.
//!
//! **What.** Flags library crates whose root file is missing those inner
//! attributes ([`CrateAttrsRuleId`]). `deny(unsafe_code)` is not enough;
//! `warn`/`deny`/`forbid(missing_docs)` all satisfy the docs lint.
//!
//! **Why.** Sibling `CLAUDE.md` files require both attributes on `lib.rs`
//! so the whole library is locked down. `[lib] path` is honored; bin-only
//! packages are skipped. `[crate_attrs] allow_unsafe` lists members that
//! may use `unsafe` (an FFI crate) without turning the rule off everywhere.
//!
//! **How to use.** Run `cordial quality` (feature `crate_attrs`). Knobs
//! live under `[crate_attrs]` in `cordial.toml`. Artifacts:
//! `{store}/findings/crate-attrs.checklist.md` and `crate-attrs-summary.md`.
//! `cordial quality --apply` writes the missing inner attributes onto each
//! library root (`--dry-run` logs without writing). Register
//! [`CRATE_ATTRS_ETIQUETTE`].
//!
//! Policy: `docs/planning/crate-attrs-etiquette.md`.

mod apply;
mod assessor;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod types;

pub use apply::{CrateAttrsApplySummary, run_crate_attrs_apply};
pub use assessor::CrateAttrsAssessor;
pub use enricher::CrateAttrsInventoryEnricher;
pub use probe::CrateAttrsSiteProbe;
pub use reporter::{CrateAttrsChecklistReporter, CrateAttrsCsvReporter, CrateAttrsSummaryReporter};
pub use scan::{library_root_rs, scan_crate_attrs};
pub use types::{CrateAttrsRuleId, CrateAttrsSiteRecord};

use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
    StaticQualityEtiquette, count_open_rule,
};
use crate::objects::Finding;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

use tracing::instrument;

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static CRATE_ATTRS_INVENTORY: CrateAttrsInventoryEnricher = CrateAttrsInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static CRATE_ATTRS_PROBE: CrateAttrsSiteProbe = CrateAttrsSiteProbe;
static CRATE_ATTRS_ASSESSOR: CrateAttrsAssessor = CrateAttrsAssessor;
static CRATE_ATTRS_CSV: CrateAttrsCsvReporter = CrateAttrsCsvReporter;
static CRATE_ATTRS_CHECKLIST: CrateAttrsChecklistReporter = CrateAttrsChecklistReporter;
static CRATE_ATTRS_SUMMARY: CrateAttrsSummaryReporter = CrateAttrsSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] =
    &[&SCOPE_ENRICHER, &CRATE_ATTRS_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&'static dyn crate::Probe] = &[&CRATE_ATTRS_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&CRATE_ATTRS_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &CRATE_ATTRS_CSV,
    &CRATE_ATTRS_CHECKLIST,
    &CRATE_ATTRS_SUMMARY,
];

/// Built-in crate-attributes etiquette: forbid unsafe and warn missing docs
/// on each library root.
pub static CRATE_ATTRS_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette::new(
    StaticEtiquette::new(
        "crate_attrs",
        "Crate attributes",
        EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
        false,
        EtiquetteExplain::new(
            "Does each library root forbid(unsafe_code) and warn(missing_docs)?",
            "Sibling CLAUDE.md files require both attributes on lib.rs so the whole library is locked down.",
            "Flags library crates whose root file is missing those inner attributes. deny(unsafe_code) is not enough; warn/deny/forbid(missing_docs) all satisfy the docs lint. [lib] path is honored; bin-only packages are skipped. [crate_attrs] allow_unsafe lists members that may use unsafe. cordial quality --apply writes the missing inner attributes.",
            "`[crate_attrs] enabled = false` in cordial.toml.",
            &[
                EtiquetteRuleExplain::new(
                    "CRATE-FORBID-UNSAFE-001",
                    "Library root missing #![forbid(unsafe_code)]",
                ),
                EtiquetteRuleExplain::new(
                    "CRATE-MISSING-DOCS-001",
                    "Library root missing #![warn(missing_docs)]",
                ),
            ],
        ),
    ),
    Some(QualityAreaSpec::new(
        "Crate attributes",
        "crate-attrs.checklist.md",
        "crate-attrs-summary.md",
        quality_area_compute,
    )),
);

#[instrument(level = "debug", skip(findings))]
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let forbid_unsafe = count_open_rule(findings, "CRATE-FORBID-UNSAFE-001");
    let missing_docs = count_open_rule(findings, "CRATE-MISSING-DOCS-001");
    let total = forbid_unsafe + missing_docs;
    (
        total,
        format!(
            "missing `forbid(unsafe_code)` **{forbid_unsafe}**, missing `warn(missing_docs)` **{missing_docs}**"
        ),
    )
}
