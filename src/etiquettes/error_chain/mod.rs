//! Whether error conversions keep `source()`.
//!
//! **What.** Among inventoried error sites, flags converters (especially
//! `map_err`) that drop the original error instead of wrapping it. Sites that
//! already preserve the chain are the contrast set, not the checklist.
//!
//! **Why.** A typed crate error is useless in the field if the foreign cause
//! was stringified away. Chain preservation is the difference between
//! “something failed” and a diagnosable `source()` walk.
//!
//! **How to use.** Run `cordial quality` (feature `error_chain`). Artifacts:
//! `{store}/findings/error-chain-preserved.checklist.md` and
//! `error-chain-preserved-summary.md`. Contrast with `foreign_error_types`
//! (breaks on foreign `E`). Register [`ERROR_CHAIN_ETIQUETTE`].
//!
//! Policy: `docs/planning/error-handling-as-plugin.md`.

mod assessor;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::ErrorChainAssessor;
pub use probe::ErrorChainProbe;
pub use reporter::{ErrorChainChecklistReporter, ErrorChainCsvReporter, ErrorChainSummaryReporter};
pub use scan::{scan_crate_error_chain, scan_rust_source};
pub use types::{ErrorChainProbeId, ErrorChainRecord, probe_counts};

use crate::SourceLoader;
use crate::enricher::ERROR_IR_ENRICHERS;
use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, StaticEtiquette, StaticQualityEtiquette,
};

static SOURCE_LOADER: SourceLoader = SourceLoader;
static ERROR_CHAIN_PROBE: ErrorChainProbe = ErrorChainProbe;
static ERROR_CHAIN_ASSESSOR: ErrorChainAssessor = ErrorChainAssessor;
static ERROR_CHAIN_CSV: ErrorChainCsvReporter = ErrorChainCsvReporter;
static ERROR_CHAIN_CHECKLIST: ErrorChainChecklistReporter = ErrorChainChecklistReporter;
static ERROR_CHAIN_SUMMARY: ErrorChainSummaryReporter = ErrorChainSummaryReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] = ERROR_IR_ENRICHERS;
static PROBES: &[&'static dyn crate::Probe] = &[&ERROR_CHAIN_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&ERROR_CHAIN_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &ERROR_CHAIN_CSV,
    &ERROR_CHAIN_CHECKLIST,
    &ERROR_CHAIN_SUMMARY,
];

/// Built-in error chain preservation etiquette bundle.
pub static ERROR_CHAIN_ETIQUETTE: StaticQualityEtiquette = StaticQualityEtiquette::new(
    StaticEtiquette::new(
        "error_chain",
        "Error chain preservation",
        EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
        false,
        EtiquetteExplain::new(
            "Which converters drop source() instead of wrapping the original error?",
            "A typed crate error is useless in the field if the foreign cause was stringified away. Chain preservation is the difference between “something failed” and a diagnosable source() walk.",
            "Among inventoried error sites, flags converters (especially map_err) that drop the original error instead of wrapping it. Sites that already preserve the chain are the contrast set, not the checklist. Reference-only / contrast inventory: no dedicated quality-report area of its own.",
            "`[error_chain] enabled = false` in cordial.toml.",
            &[
                EtiquetteRuleExplain::new(
                    "ERROR-CHAIN-WRAPPER-SOURCE-001",
                    "Wrapper should keep a source field",
                ),
                EtiquetteRuleExplain::new(
                    "ERROR-CHAIN-KIND-WRAPPER-PAYLOAD-001",
                    "Kind wrapper payload shape",
                ),
                EtiquetteRuleExplain::new(
                    "ERROR-CHAIN-FROM-BRIDGE-001",
                    "From bridge drops the cause",
                ),
                EtiquetteRuleExplain::new(
                    "ERROR-CHAIN-PRESERVED-QUESTION-MARK-001",
                    "Contrast: ? already preserves the chain",
                ),
                EtiquetteRuleExplain::new(
                    "ERROR-CHAIN-PRESERVED-MAP-ERR-001",
                    "Contrast: map_err already preserves the chain",
                ),
            ],
        ),
    ),
    None,
);
