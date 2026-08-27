//! Classified `tracing::instrument` coverage.
//!
//! **What.** Every function gets a use-class (`FunctionRole`), a complexity,
//! and a target `InstrumentRecipe`. Probes flag a missing attribute, a recipe
//! delta (`level`, `err`, `ret`, `fields`, `skip`), or **attenuation**:
//! `#[instrument]` already present on proof-only code, skip-policy files, or
//! ungated on a prover-reachable function. Apply writes, gates, or removes
//! to match. Visibility does not exempt a function.
//!
//! **Why.** A missing-span census treats constructors, getters, scanners, and
//! entry points the same. Skipping private helpers creates blind spots in the
//! internals. The etiquette’s job is to instrument each function properly for
//! its class. Volume is a subscriber `level` problem, not a reason to skip
//! spans. `Fallible` means the function returns `Result` or a `*Result` alias;
//! `?` on `Option` is not fallible.
//!
//! **How to use.**
//! 1. `cordial quality` writes `{store}/findings/tracing-instrument.checklist.md`
//!    and `tracing-summary.md`. Subscriber-init rows go to
//!    `tracing-subscriber.checklist.md` (`--apply` does not patch those).
//! 2. `cordial quality --apply` (or `--dry-run`) patches open instrument
//!    checklist rows. Re-run quality after apply.
//!
//! Knobs live under `[tracing]` in `cordial.toml` (`extra_skip`,
//! `apply_gate_crates`, `apply_skip_crates`, `[tracing.subscriber]`).
//! Role→level maps stay in code. Feature `tracing` is on by default. Register
//! [`TRACING_ETIQUETTE`] on a [`crate::Session`].
//!
//! Policy: `docs/planning/tracing-etiquette.md`.

mod apply;
mod assessor;
mod call_graph;
mod classify;
mod delta;
mod display_types;
mod enricher;
mod present;
mod probe;
mod quality_area;
mod recipe;
mod recordable;
mod reporter;
mod scan;
mod subscriber;
mod types;

pub use apply::{
    InstrumentApplySummary, InstrumentGap, parse_tracing_instrument_checklist,
    parse_tracing_instrument_checklist_text, run_tracing_instrument_apply,
};

pub use assessor::TracingAssessor;
pub use enricher::FunctionInventoryEnricher;
pub use probe::{ForbiddenInstrumentProbe, MissingInstrumentProbe, RecipeDeltaProbe};
pub use reporter::{TracingChecklistReporter, TracingCsvReporter, TracingSummaryReporter};
pub use scan::scan_rust_source;
pub use subscriber::{SubscriberRuleId, SubscriberSiteRecord, scan_crate_tracing_subscriber};

use crate::etiquette::{QualityAreaSpec, StaticEtiquette, StaticQualityEtiquette};
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static FUNCTION_INVENTORY: FunctionInventoryEnricher = FunctionInventoryEnricher;
static SUBSCRIBER_INVENTORY: subscriber::SubscriberInventoryEnricher =
    subscriber::SubscriberInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static MISSING_INSTRUMENT_PROBE: MissingInstrumentProbe = MissingInstrumentProbe;
static RECIPE_DELTA_PROBE: RecipeDeltaProbe = RecipeDeltaProbe;
static FORBIDDEN_INSTRUMENT_PROBE: ForbiddenInstrumentProbe = ForbiddenInstrumentProbe;
static SUBSCRIBER_PROBE: subscriber::SubscriberSiteProbe = subscriber::SubscriberSiteProbe;
static TRACING_ASSESSOR: TracingAssessor = TracingAssessor;
static SUBSCRIBER_ASSESSOR: subscriber::SubscriberAssessor = subscriber::SubscriberAssessor;
static TRACING_CSV: TracingCsvReporter = TracingCsvReporter;
static TRACING_CHECKLIST: TracingChecklistReporter = TracingChecklistReporter;
static TRACING_SUMMARY: TracingSummaryReporter = TracingSummaryReporter;
static SUBSCRIBER_CSV: subscriber::SubscriberCsvReporter = subscriber::SubscriberCsvReporter;
static SUBSCRIBER_CHECKLIST: subscriber::SubscriberChecklistReporter =
    subscriber::SubscriberChecklistReporter;
static SUBSCRIBER_SUMMARY: subscriber::SubscriberSummaryReporter =
    subscriber::SubscriberSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] = &[
    &SCOPE_ENRICHER,
    &FUNCTION_INVENTORY,
    &SUBSCRIBER_INVENTORY,
    &ATTRIBUTE_ENRICHER,
];
static PROBES: &[&'static dyn crate::Probe] = &[
    &MISSING_INSTRUMENT_PROBE,
    &RECIPE_DELTA_PROBE,
    &FORBIDDEN_INSTRUMENT_PROBE,
    &SUBSCRIBER_PROBE,
];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&TRACING_ASSESSOR, &SUBSCRIBER_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &TRACING_CSV,
    &TRACING_CHECKLIST,
    &TRACING_SUMMARY,
    &SUBSCRIBER_CSV,
    &SUBSCRIBER_CHECKLIST,
    &SUBSCRIBER_SUMMARY,
];

/// Built-in tracing instrument etiquette bundle.
pub static TRACING_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette {
    etiquette: StaticEtiquette {
        id: "tracing",
        name: "Tracing instrument",
        loaders: LOADERS,
        enrichers: ENRICHERS,
        probes: PROBES,
        assessors: ASSESSORS,
        workspace_assessors: None,
        reporters: REPORTERS,
        is_coverage: false,
    },
    quality_area: Some(QualityAreaSpec {
        title: "Tracing instrumentation",
        checklist: "tracing-instrument.checklist.md",
        summary: "tracing-summary.md",
        compute: quality_area::quality_area_compute,
    }),
};
