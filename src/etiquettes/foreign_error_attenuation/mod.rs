//! How foreign error sites should be handled.
//!
//! **What.** Classifies each typed foreign site
//! ([`ForeignErrorHandlingClass`]): chain already preserved, chain break,
//! pending infrastructure, or neutral. Suggests a resolution (keep the
//! exemplar, replace a stringifying `map_err`, add infrastructure then `?`,
//! or review by hand).
//!
//! **Why.** Listing foreign types is not enough; the actionable question is
//! *what to do at this site*. Attenuation turns the census into a queue:
//! wrap into the internal type, wait for a `From` impl, or leave a documented
//! exception.
//!
//! **How to use.** Run `cordial quality` (feature
//! `foreign_error_attenuation`). Artifacts:
//! `{store}/findings/foreign-error-attenuation.checklist.md` and
//! `foreign-error-attenuation-summary.md`. Register
//! [`FOREIGN_ERROR_ATTENUATION_ETIQUETTE`].
//!
//! Policy: `docs/planning/error-handling-as-plugin.md`.

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
use crate::etiquette::{StaticEtiquette, StaticQualityEtiquette};

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
pub static FOREIGN_ERROR_ATTENUATION_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette {
    etiquette: StaticEtiquette {
        id: "foreign_error_attenuation",
        name: "Foreign error attenuation",
        loaders: LOADERS,
        enrichers: ENRICHERS,
        probes: PROBES,
        assessors: ASSESSORS,
        workspace_assessors: None,
        reporters: REPORTERS,
        is_coverage: false,
    },
    // Declines a dedicated row on purpose: its migration-backlog
    // (chain-break + pending-infra) counts feed the hand-composed
    // "Error handling" area instead (see reporter::quality_report).
    quality_area: None,
};
