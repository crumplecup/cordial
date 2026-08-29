//! Inventory of `Result` control-flow sites.
//!
//! **What.** Records `?`, `map_err`, `return Err`, `if let Err`, `match` on
//! `Err`, and `ok_or` ([`ErrorSiteKind`]). Downstream etiquettes partition
//! those rows by origin (internal vs foreign).
//!
//! **Why.** You cannot judge chain preservation or foreign attenuation until
//! every error site is named. This is the census layer of the error-handling
//! plugin; later layers consume the same IR.
//!
//! **How to use.** Run `cordial quality` (feature `error_sites`). Artifacts:
//! `{store}/findings/error-sites.checklist.md`, `error-sites-summary.md`,
//! partition CSV/summary. Register [`ERROR_SITES_ETIQUETTE`] on a
//! [`crate::Session`]. Shares the `error_ir` scan.
//!
//! Policy: `docs/planning/error-handling-as-plugin.md`.

mod assessor;
mod foreign_infer;
mod partition;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::ErrorSiteAssessor;
pub use foreign_infer::{ForeignTypeConfidence, infer_foreign_error_type};
pub use partition::{
    PartitionedErrorSiteRow, partition_error_site_records, partition_error_site_row,
};
pub use probe::ErrorSiteProbe;
pub use reporter::{
    ErrorSitesChecklistReporter, ErrorSitesCsvReporter, ErrorSitesPartitionSummaryReporter,
    ErrorSitesPartitionedCsvReporter, ErrorSitesSummaryReporter,
};
pub use scan::{scan_crate_error_sites, scan_rust_source};
pub use types::{
    ErrorOriginClass, ErrorSiteKind, ErrorSiteRecord, ErrorSiteScanRow, ForeignErrorRecordKind,
};

use crate::SourceLoader;
use crate::enricher::ERROR_IR_ENRICHERS;
use crate::etiquette::{
    EtiquetteExplain, EtiquetteRuleExplain, StaticEtiquette, StaticQualityEtiquette,
};

static SOURCE_LOADER: SourceLoader = SourceLoader;
static ERROR_SITE_PROBE: ErrorSiteProbe = ErrorSiteProbe;
static ERROR_SITE_ASSESSOR: ErrorSiteAssessor = ErrorSiteAssessor;
static ERROR_SITES_CSV: ErrorSitesCsvReporter = ErrorSitesCsvReporter;
static ERROR_SITES_CHECKLIST: ErrorSitesChecklistReporter = ErrorSitesChecklistReporter;
static ERROR_SITES_SUMMARY: ErrorSitesSummaryReporter = ErrorSitesSummaryReporter;
static ERROR_SITES_PARTITIONED_CSV: ErrorSitesPartitionedCsvReporter =
    ErrorSitesPartitionedCsvReporter;
static ERROR_SITES_PARTITION_SUMMARY: ErrorSitesPartitionSummaryReporter =
    ErrorSitesPartitionSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] = ERROR_IR_ENRICHERS;
static PROBES: &[&'static dyn crate::Probe] = &[&ERROR_SITE_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&ERROR_SITE_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &ERROR_SITES_CSV,
    &ERROR_SITES_CHECKLIST,
    &ERROR_SITES_SUMMARY,
    &ERROR_SITES_PARTITIONED_CSV,
    &ERROR_SITES_PARTITION_SUMMARY,
];

/// Built-in error sites etiquette bundle.
pub static ERROR_SITES_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette {
    etiquette: StaticEtiquette {
        id: "error_sites",
        name: "Error sites",
        loaders: LOADERS,
        enrichers: ENRICHERS,
        probes: PROBES,
        assessors: ASSESSORS,
        workspace_assessors: None,
        reporters: REPORTERS,
        is_coverage: false,
        explain: EtiquetteExplain {
            summary: "Where are ?, map_err, and related error sites?",
            why: "You cannot judge chain preservation or foreign attenuation until every error site is named. This is the census layer; later layers consume the same IR.",
            logic: "Records ?, map_err, return Err, if let Err, match on Err, and ok_or. Downstream etiquettes partition those rows by origin (internal vs foreign). Reference-only inventory: no dedicated quality-report area.",
            opt_out: "`[error_sites] enabled = false` in cordial.toml.",
            rules: &[
                EtiquetteRuleExplain {
                    id: "ERROR-SITE-QUESTION-MARK",
                    summary: "`?` site",
                },
                EtiquetteRuleExplain {
                    id: "ERROR-SITE-MAP-ERR",
                    summary: "`map_err` site",
                },
                EtiquetteRuleExplain {
                    id: "ERROR-SITE-RETURN-ERR",
                    summary: "`return Err` site",
                },
                EtiquetteRuleExplain {
                    id: "ERROR-SITE-IF-LET-ERR",
                    summary: "`if let Err` site",
                },
                EtiquetteRuleExplain {
                    id: "ERROR-SITE-MATCH-ERR",
                    summary: "`match` on Err",
                },
                EtiquetteRuleExplain {
                    id: "ERROR-SITE-OK-OR",
                    summary: "`ok_or` site",
                },
            ],
        },
    },
    // Declines a dedicated row on purpose: an intermediate census (its
    // own doc comment: "Resolution strategies are out of scope"),
    // feeding error_chain/foreign_error_types/foreign_error_attenuation
    // rather than being itself an action-item area.
    quality_area: None,
};
