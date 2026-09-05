//! Manual patterns that a derive crate would write.
//!
//! **What.** Finds hand-written boilerplate that can move back to the type
//! definition.
//!
//! **Why.** Repeated accessors and builders are noise. Derives keep the type
//! definition as the source of truth and shrink the surface tracing and
//! visibility have to classify.
//!
//! **Flags.** Hand-rolled builders, constructors that should be builders,
//! getters, setters, `as_ref` / `as_str`, trivial `new`, and public fields.
//! `Some(arg)` and `arg.into()` are `derive_setters` options, not exemptions.
//!
//! **Ignores.** Error constructors are exempt from `derive_new` when they use
//! `#[track_caller]`. Clap schema types skip public-field linting.
//! `const fn` constructors, getters, setters, and `as_ref` / `as_str`
//! forwarders are exempt because the recommended derives would drop
//! const-evaluability.
//!
//! **Outputs.** `{store}/findings/derives.checklist.md`,
//! `derives-summary.md`, and CSV.
//!
//! **Config.** `[derives]` owns the thresholds in
//! [`crate::config::DerivesThresholds`]. `[derives] enabled = false` opts out.
//! Exceptions are managed with `cordial exceptions show derives`. Register
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
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
    StaticQualityEtiquette, count_open_rule,
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
pub static DERIVES_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette::new(
    StaticEtiquette::new(
        "derives",
        "Derive patterns",
        EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
        false,
        EtiquetteExplain::new(
            "Which manual builders, getters, setters, or new could be derives?",
            "Repeated accessors and builders are noise. Derives keep the type definition as the source of truth.",
            "Flags hand-rolled builders, constructors that should be builders, getters, setters, as_ref/as_str, trivial new, and public fields. Error types are exempt from derive_new (#[track_caller]). Clap Parser/Args/Subcommand skip public-field linting. const fn constructors and accessors are exempt because the derive crates do not generate const fn. Knobs: [derives] in cordial.toml.",
            "`[derives] enabled = false` in cordial.toml.",
            &[
                EtiquetteRuleExplain::new("DERIVE-BUILDER-001", "Hand-rolled builder"),
                EtiquetteRuleExplain::new(
                    "DERIVE-USE-BUILDER-001",
                    "Constructor arity says use a builder",
                ),
                EtiquetteRuleExplain::new("DERIVE-GETTER-001", "Hand-rolled getter"),
                EtiquetteRuleExplain::new("DERIVE-SETTER-001", "Hand-rolled setter"),
                EtiquetteRuleExplain::new("DERIVE-ASREF-001", "Hand-rolled as_ref"),
                EtiquetteRuleExplain::new("DERIVE-ASSTR-001", "Hand-rolled as_str"),
                EtiquetteRuleExplain::new("DERIVE-NEW-001", "Trivial new that could be derive_new"),
                EtiquetteRuleExplain::new(
                    "DERIVE-PUB-FIELD-001",
                    "Public field that should stay private",
                ),
            ],
        ),
    ),
    Some(QualityAreaSpec::new(
        "Derive patterns",
        "derives.checklist.md",
        "derives-summary.md",
        quality_area_compute,
    )),
);

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
