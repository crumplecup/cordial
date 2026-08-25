//! Manual patterns that a derive crate would write.
//!
//! **What.** Flags hand-rolled builders, constructors that should be
//! builders, getters, setters (`into` / `strip_option`), `as_ref` / `as_str`,
//! trivial `new`, and public fields ([`DeriveRuleId`]). Policy knobs live in
//! [`crate::config::DerivesThresholds`] / `[derives]` in `cordial.toml`.
//!
//! **Why.** Repeated accessors and builders are noise. Derives keep the type
//! definition as the source of truth and shrink the surface tracing and
//! visibility have to classify. Error types are exempt from `derive_new`
//! because their constructors use `#[track_caller]`. Clap `Parser` /
//! `Args` / `Subcommand` types skip public-field linting (CLI schema).
//! `const fn` constructors, getters, setters, and `as_ref`/`as_str`
//! forwarders are exempt from their respective rules: none of
//! `derive_new::new`, `derive_getters::Getters`, `derive_setters::Setters`,
//! or `derive_more::AsRef` generate `const fn` output (confirmed against
//! each crate's own docs, not assumed), so recommending one of them would
//! recommend a lossy change -- silently dropping const-evaluability with
//! no compiler warning, since nothing forces a call site to already need
//! it. `DERIVE-USE-BUILDER-001` gets the same exemption for the same
//! reason: `derive_builder::Builder`'s generated `build()` isn't const
//! either. This etiquette asks
//! *could this be a derive?* (or *should this constructor be a builder?*);
//! tracing asks *is this function instrumented for its role?* `Some(arg)`
//! and `arg.into()` are `derive_setters` options, not exemptions. `as_str`
//! / `as_ref` steer to `derive_more::AsRef`.
//!
//! **How to use.** Run `cordial quality` (feature `derives`). Artifacts:
//! `{store}/findings/derives.checklist.md`, `derives-summary.md`, and CSV.
//! Exceptions: `cordial exceptions show derives`. Register
//! [`DERIVES_ETIQUETTE`] on a [`crate::Session`].

mod assessor;
mod enricher;
mod path_inclusion;
mod probe;
mod reporter;
mod scan;
mod syntax;
mod types;

pub use assessor::DeriveAssessor;
pub use enricher::DeriveInventoryEnricher;
pub use path_inclusion::{PathInclusionFacts, workspace_path_inclusions};
pub use probe::DeriveSiteProbe;
pub use reporter::{DeriveChecklistReporter, DeriveCsvReporter, DeriveSummaryReporter};
pub use scan::scan_rust_source;
pub use types::{DeriveRuleId, DeriveSiteRecord};

use crate::etiquette::{
    QualityAreaSpec, StaticEtiquette, StaticQualityEtiquette, count_open_rule,
};
use crate::objects::Finding;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

use tracing::instrument;

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static DERIVE_INVENTORY: DeriveInventoryEnricher = DeriveInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static DERIVE_PROBE: DeriveSiteProbe = DeriveSiteProbe;
static DERIVE_ASSESSOR: DeriveAssessor = DeriveAssessor;
static DERIVE_CSV: DeriveCsvReporter = DeriveCsvReporter;
static DERIVE_CHECKLIST: DeriveChecklistReporter = DeriveChecklistReporter;
static DERIVE_SUMMARY: DeriveSummaryReporter = DeriveSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] =
    &[&SCOPE_ENRICHER, &DERIVE_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&'static dyn crate::Probe] = &[&DERIVE_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&DERIVE_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] =
    &[&DERIVE_CSV, &DERIVE_CHECKLIST, &DERIVE_SUMMARY];

/// Built-in derives etiquette bundle.
pub static DERIVES_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette {
    etiquette: StaticEtiquette {
        id: "derives",
        name: "Derive patterns",
        loaders: LOADERS,
        enrichers: ENRICHERS,
        probes: PROBES,
        assessors: ASSESSORS,
        workspace_assessors: None,
        reporters: REPORTERS,
        is_coverage: false,
    },
    quality_area: Some(QualityAreaSpec {
        title: "Derive patterns",
        checklist: "derives.checklist.md",
        summary: "derives-summary.md",
        compute: quality_area_compute,
    }),
};

#[instrument(level = "debug", skip(findings))]
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let builder = count_open_rule(findings, "DERIVE-BUILDER-001");
    let use_builder = count_open_rule(findings, "DERIVE-USE-BUILDER-001");
    let getter = count_open_rule(findings, "DERIVE-GETTER-001");
    let setter = count_open_rule(findings, "DERIVE-SETTER-001");
    let as_ref = count_open_rule(findings, "DERIVE-ASREF-001");
    let as_str = count_open_rule(findings, "DERIVE-ASSTR-001");
    let new = count_open_rule(findings, "DERIVE-NEW-001");
    let pub_field = count_open_rule(findings, "DERIVE-PUB-FIELD-001");
    let total = builder + use_builder + getter + setter + as_ref + as_str + new + pub_field;
    let detail = format!(
        "builder **{builder}**, use_builder **{use_builder}**, getter **{getter}**, \
         setter **{setter}**, as_ref **{as_ref}**, as_str **{as_str}**, new **{new}**, \
         pub_field **{pub_field}**"
    );
    (total, detail)
}
