//! How foreign error sites should be handled.
//!
//! **What.** Classifies each typed foreign site into an actionable handling
//! class.
//!
//! **Why.** Listing foreign types is not enough; the operational question is
//! what to do at this site. Attenuation turns the census into a queue: wrap
//! into the internal type, wait for a `From` impl, or leave a documented
//! exception.
//!
//! **Flags.** [`ForeignErrorHandlingClass`] rows: chain already preserved,
//! chain break, pending infrastructure, or neutral. Resolutions say to keep
//! the exemplar, replace a stringifying `map_err`, add infrastructure then use
//! `?`, or review by hand.
//!
//! **Ignores.** Pure type-surface inventory belongs to `foreign_error_types`.
//! Internal architecture compliance belongs to `internal_error_chain`.
//!
//! **Outputs.** `{store}/findings/foreign-error-attenuation.checklist.md`,
//! `foreign-error-attenuation-summary.md`, and CSV.
//!
//! **Config.** `[foreign_error_attenuation] enabled = false` opts out in
//! `cordial.toml`. Register [`FOREIGN_ERROR_ATTENUATION_ETIQUETTE`]. Policy:
//! `docs/planning/error-handling-as-plugin.md`.

mod assess;
mod assessor;
mod enricher;
mod probe;
mod reporter;
mod types;

pub use assess::build_foreign_error_attenuation_report;
pub use assessor::ForeignErrorAttenuationAssessor;
pub use enricher::ForeignErrorAttenuationInventoryEnricher;
pub use probe::ForeignErrorAttenuationProbe;
pub use reporter::{
    ForeignErrorAttenuationChecklistReporter, ForeignErrorAttenuationCsvReporter,
    ForeignErrorAttenuationSummaryReporter,
};
pub use types::{
    ForeignErrorAttenuationReport, ForeignErrorHandlingClass,
    WorkspaceForeignErrorAttenuationSummary, build_workspace_foreign_error_attenuation_summary,
};

use crate::SourceLoader;
use crate::enricher::ERROR_IR_ENRICHERS;
use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, StaticEtiquette, StaticQualityEtiquette,
};

static SOURCE_LOADER: SourceLoader = SourceLoader;
static FOREIGN_ERROR_ATTENUATION_PROBE: ForeignErrorAttenuationProbe = ForeignErrorAttenuationProbe;
static FOREIGN_ERROR_ATTENUATION_ASSESSOR: ForeignErrorAttenuationAssessor =
    ForeignErrorAttenuationAssessor;
static FOREIGN_ERROR_ATTENUATION_CSV: ForeignErrorAttenuationCsvReporter =
    ForeignErrorAttenuationCsvReporter;
static FOREIGN_ERROR_ATTENUATION_CHECKLIST: ForeignErrorAttenuationChecklistReporter =
    ForeignErrorAttenuationChecklistReporter;
static FOREIGN_ERROR_ATTENUATION_SUMMARY: ForeignErrorAttenuationSummaryReporter =
    ForeignErrorAttenuationSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] = ERROR_IR_ENRICHERS;
static PROBES: &[&'static dyn crate::Probe] = &[&FOREIGN_ERROR_ATTENUATION_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&FOREIGN_ERROR_ATTENUATION_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &FOREIGN_ERROR_ATTENUATION_CSV,
    &FOREIGN_ERROR_ATTENUATION_CHECKLIST,
    &FOREIGN_ERROR_ATTENUATION_SUMMARY,
];

/// Built-in foreign error attenuation etiquette bundle.
pub static FOREIGN_ERROR_ATTENUATION_ETIQUETTE: StaticQualityEtiquette =
    StaticQualityEtiquette::new(
        StaticEtiquette::new(
            "foreign_error_attenuation",
            "Foreign error attenuation",
            EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
            false,
            EtiquetteExplain::new(
                "How should those foreign error sites be wrapped, mapped, or deferred?",
                "Listing foreign types is not enough; the actionable question is what to do at this site.",
                "Classifies each typed foreign site: chain already preserved, chain break, pending infrastructure, or neutral. Suggests keep the exemplar, replace a stringifying map_err, add infrastructure then ?, or review by hand. Feeds the hand-composed Error handling quality-report area.",
                "`[foreign_error_attenuation] enabled = false` in cordial.toml.",
                &[
                    EtiquetteRuleExplain::new(
                        "ERROR-HANDLING-CHAIN-PRESERVED",
                        "Contrast: chain already preserved",
                    ),
                    EtiquetteRuleExplain::new(
                        "ERROR-HANDLING-CHAIN-BREAK",
                        "Site drops the source() chain",
                    ),
                    EtiquetteRuleExplain::new(
                        "ERROR-HANDLING-PENDING-INFRA",
                        "Needs a From / wrapper before ?",
                    ),
                    EtiquetteRuleExplain::new(
                        "ERROR-HANDLING-NEUTRAL",
                        "Review by hand; not auto-classified",
                    ),
                ],
            ),
        ),
        None,
    );
