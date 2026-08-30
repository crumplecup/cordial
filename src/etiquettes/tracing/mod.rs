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
//!    `tracing-subscriber.checklist.md`. Leftover stdio (`println!`/`print!`/
//!    `dbg!`, including `main`, `src/cli`, and `tests/`) go to
//!    `tracing-print.checklist.md`. Filter those with `[tracing.stdio]`
//!    (`--apply` does not patch those).
//! 2. `cordial quality --apply` (or `--dry-run`) patches open instrument
//!    checklist rows. Re-run quality after apply.
//!
//! Knobs live under `[tracing]` in `cordial.toml` (`extra_skip`,
//! `apply_gate_crates`, `apply_skip_crates`, `[tracing.subscriber]`,
//! `[tracing.stdio]`).
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
mod print;
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
pub use print::{
    PrintRuleId, PrintSiteRecord, scan_crate_tracing_print,
    scan_rust_source as scan_tracing_print_rust_source,
};
pub use probe::{ForbiddenInstrumentProbe, MissingInstrumentProbe, RecipeDeltaProbe};
pub use reporter::{TracingChecklistReporter, TracingCsvReporter, TracingSummaryReporter};
pub use scan::scan_rust_source;
pub use subscriber::{SubscriberRuleId, SubscriberSiteRecord, scan_crate_tracing_subscriber};

use crate::etiquette::{
    EtiquetteExplain, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
    StaticQualityEtiquette,
};
use crate::{AttributeEnricher, ScopeEnricher, SourceLoader};

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static FUNCTION_INVENTORY: FunctionInventoryEnricher = FunctionInventoryEnricher;
static SUBSCRIBER_INVENTORY: subscriber::SubscriberInventoryEnricher =
    subscriber::SubscriberInventoryEnricher;
static PRINT_INVENTORY: print::PrintInventoryEnricher = print::PrintInventoryEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static MISSING_INSTRUMENT_PROBE: MissingInstrumentProbe = MissingInstrumentProbe;
static RECIPE_DELTA_PROBE: RecipeDeltaProbe = RecipeDeltaProbe;
static FORBIDDEN_INSTRUMENT_PROBE: ForbiddenInstrumentProbe = ForbiddenInstrumentProbe;
static SUBSCRIBER_PROBE: subscriber::SubscriberSiteProbe = subscriber::SubscriberSiteProbe;
static PRINT_PROBE: print::PrintSiteProbe = print::PrintSiteProbe;
static TRACING_ASSESSOR: TracingAssessor = TracingAssessor;
static SUBSCRIBER_ASSESSOR: subscriber::SubscriberAssessor = subscriber::SubscriberAssessor;
static PRINT_ASSESSOR: print::PrintAssessor = print::PrintAssessor;
static TRACING_CSV: TracingCsvReporter = TracingCsvReporter;
static TRACING_CHECKLIST: TracingChecklistReporter = TracingChecklistReporter;
static TRACING_SUMMARY: TracingSummaryReporter = TracingSummaryReporter;
static SUBSCRIBER_CSV: subscriber::SubscriberCsvReporter = subscriber::SubscriberCsvReporter;
static SUBSCRIBER_CHECKLIST: subscriber::SubscriberChecklistReporter =
    subscriber::SubscriberChecklistReporter;
static SUBSCRIBER_SUMMARY: subscriber::SubscriberSummaryReporter =
    subscriber::SubscriberSummaryReporter;
static PRINT_CSV: print::PrintCsvReporter = print::PrintCsvReporter;
static PRINT_CHECKLIST: print::PrintChecklistReporter = print::PrintChecklistReporter;
static PRINT_SUMMARY: print::PrintSummaryReporter = print::PrintSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] = &[
    &SCOPE_ENRICHER,
    &FUNCTION_INVENTORY,
    &SUBSCRIBER_INVENTORY,
    &PRINT_INVENTORY,
    &ATTRIBUTE_ENRICHER,
];
static PROBES: &[&'static dyn crate::Probe] = &[
    &MISSING_INSTRUMENT_PROBE,
    &RECIPE_DELTA_PROBE,
    &FORBIDDEN_INSTRUMENT_PROBE,
    &SUBSCRIBER_PROBE,
    &PRINT_PROBE,
];
static ASSESSORS: &[&'static dyn crate::Assessor] =
    &[&TRACING_ASSESSOR, &SUBSCRIBER_ASSESSOR, &PRINT_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &TRACING_CSV,
    &TRACING_CHECKLIST,
    &TRACING_SUMMARY,
    &SUBSCRIBER_CSV,
    &SUBSCRIBER_CHECKLIST,
    &SUBSCRIBER_SUMMARY,
    &PRINT_CSV,
    &PRINT_CHECKLIST,
    &PRINT_SUMMARY,
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
        explain: EtiquetteExplain {
            summary: "Are functions instrumented with the recipe for their role?",
            why: "A missing-span census that skips private helpers creates blind spots. Volume is a subscriber level problem, not a reason to skip spans.",
            logic: "Every function gets a use-class, complexity, and target InstrumentRecipe. Probes flag a missing attribute, a recipe delta, or attenuation (instrument on proof-only code, skip-policy files, or ungated on a prover-reachable function). Visibility does not exempt a function. Subscriber-init rows are a second checklist. Leftover stdio macros are a third filter ([tracing.stdio]: println/eprintln/print/eprint/dbg, skip_cargo_protocol, skip_folders). --apply does not patch subscriber or print rows.",
            opt_out: "`[tracing] enabled = false` in cordial.toml.",
            rules: &[
                EtiquetteRuleExplain {
                    id: "TRACING-MISSING-INSTRUMENT",
                    summary: "Function lacks #[instrument]",
                },
                EtiquetteRuleExplain {
                    id: "TRACING-LEVEL-MISMATCH",
                    summary: "level does not match the recipe",
                },
                EtiquetteRuleExplain {
                    id: "TRACING-SKIP-MISSING",
                    summary: "recipe skip list is missing",
                },
                EtiquetteRuleExplain {
                    id: "TRACING-ERR-MISSING",
                    summary: "fallible function missing err",
                },
                EtiquetteRuleExplain {
                    id: "TRACING-ERROR-PATH-SILENT",
                    summary: "error path is not recorded",
                },
                EtiquetteRuleExplain {
                    id: "TRACING-FIELDS-MISSING",
                    summary: "recipe fields are missing",
                },
                EtiquetteRuleExplain {
                    id: "TRACING-PROOF-INSTRUMENT",
                    summary: "#[instrument] on proof-only code",
                },
                EtiquetteRuleExplain {
                    id: "TRACING-UNGATED-INSTRUMENT",
                    summary: "ungated instrument on a prover-reachable function",
                },
                EtiquetteRuleExplain {
                    id: "TRACING-SKIP-INSTRUMENT",
                    summary: "instrument present on a skip-policy file",
                },
                EtiquetteRuleExplain {
                    id: "TRACING-SUBSCRIBER-MAIN",
                    summary: "binary main has no subscriber init",
                },
                EtiquetteRuleExplain {
                    id: "TRACING-SUBSCRIBER-TEST",
                    summary: "tests have no subscriber init",
                },
                EtiquetteRuleExplain {
                    id: "TRACING-SUBSCRIBER-LIB",
                    summary: "library has no documented subscriber story",
                },
                EtiquetteRuleExplain {
                    id: "TRACING-SUBSCRIBER-RUST-LOG",
                    summary: "RUST_LOG / EnvFilter policy mismatch",
                },
                EtiquetteRuleExplain {
                    id: "TRACING-SUBSCRIBER-IDEMPOTENT",
                    summary: "init is not idempotent",
                },
                EtiquetteRuleExplain {
                    id: "TRACING-STD-PRINTLN",
                    summary: "leftover println!; use a tracing event",
                },
                EtiquetteRuleExplain {
                    id: "TRACING-STD-EPRINTLN",
                    summary: "leftover eprintln!; use a tracing event",
                },
                EtiquetteRuleExplain {
                    id: "TRACING-STD-PRINT",
                    summary: "leftover print!; use a tracing event",
                },
                EtiquetteRuleExplain {
                    id: "TRACING-STD-EPRINT",
                    summary: "leftover eprint!; use a tracing event",
                },
                EtiquetteRuleExplain {
                    id: "TRACING-STD-DBG",
                    summary: "leftover dbg!; use a tracing event",
                },
            ],
        },
    },
    quality_area: Some(QualityAreaSpec {
        title: "Tracing instrumentation",
        checklist: "tracing-instrument.checklist.md",
        summary: "tracing-summary.md",
        compute: quality_area::quality_area_compute,
    }),
};
