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
//! This etiquette asks
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
mod probe;
mod reporter;
mod scan;
mod syntax;
mod types;

pub use assessor::DeriveAssessor;
pub use enricher::DeriveInventoryEnricher;
pub use probe::DeriveSiteProbe;
pub use reporter::{DeriveChecklistReporter, DeriveCsvReporter, DeriveSummaryReporter};
pub use scan::scan_rust_source;
pub use types::{DeriveRuleId, DeriveSiteRecord};

use crate::etiquette::StaticEtiquette;
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

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
pub static DERIVES_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "derives",
    name: "Derive patterns",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: None,
    reporters: REPORTERS,
    is_coverage: false,
};
