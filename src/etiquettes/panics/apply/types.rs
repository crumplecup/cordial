//! Exact repairs and explicit unsatisfied migration obligations.

use crate::{ClosedPanicBlocker, GeneratedErrorNames};
use std::path::PathBuf;

/// An open assessed panic site, identified through its shared graph anchor.
#[derive(Debug, Clone, derive_getters::Getters, derive_new::new)]
#[new(visibility = "pub(super)")]
pub struct PanicApplySite {
    crate_name: String,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    context: String,
    rule_id: String,
}

/// One complete migration, including every signature changed along its callers.
#[derive(Debug, Clone, derive_getters::Getters, derive_new::new)]
#[new(visibility = "pub(super)")]
pub struct PanicApplyRepair {
    site: PanicApplySite,
    functions: Vec<String>,
    error_names: GeneratedErrorNames,
}

/// A reason apply left a finding available for manual repair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanicApplyBlocker {
    /// This first declaration generator implements owned error storage only.
    BorrowedPolicy,
    /// Only an exact unwrap with unique shared source coordinates is supported.
    UnsupportedSite,
    /// No Cargo main boundary was collected for this crate.
    MissingEntryPoint,
    /// A declaration, caller, written use, or boundary obligation remains open.
    Closure(ClosedPanicBlocker),
}

/// Proposed or written repairs; dry-run repairs are never worklist claims.
#[derive(Debug, Default, derive_getters::Getters)]
pub struct PanicApplySummary {
    repairs: Vec<PanicApplyRepair>,
    blockers: Vec<(PanicApplySite, PanicApplyBlocker)>,
}

impl PanicApplySummary {
    #[tracing::instrument(level = "trace", skip(self, repair))]
    pub(super) fn repair(&mut self, repair: PanicApplyRepair) {
        self.repairs.push(repair);
    }

    #[tracing::instrument(level = "trace", skip(self, site, reason))]
    pub(super) fn block(&mut self, site: PanicApplySite, reason: PanicApplyBlocker) {
        self.blockers.push((site, reason));
    }
}
