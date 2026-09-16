//! Amenable std registry coverage — std inventory vs Provenance/Witness layers.

use serde::{Deserialize, Serialize};
use tracing::instrument;

mod classify;
mod gaps;

pub use classify::{
    ClassifyRowArgs, build_amenable_ext_report, build_amenable_std_report,
    classify_amenable_ext_row, classify_amenable_std_row, resolve_alias_chain,
};
pub use gaps::{
    amenable_ext_gap_fields, amenable_gap_fields, build_amenable_ext_gaps, build_amenable_std_gaps,
};

/// Overall registration status for one std type row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmenableStdStatus {
    /// The item is fully satisfied.
    Complete,
    /// Partial.
    Partial,
    /// Expected item is absent.
    Missing,
    /// The item is out of scope for this run.
    Skipped,
}

impl std::fmt::Display for AmenableStdStatus {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Complete => write!(f, "Complete"),
            Self::Partial => write!(f, "Partial"),
            Self::Missing => write!(f, "Missing"),
            Self::Skipped => write!(f, "Skipped"),
        }
    }
}

/// One row in an amenable std registry coverage report.
#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Eq, derive_new::new, derive_getters::Getters,
)]
pub struct AmenableStdEntry {
    type_path: String,
    type_kind: String,
    #[getter(copy)]
    is_generic: bool,
    #[getter(copy)]
    evidence_link: bool,
    evidence_name: Option<String>,
    #[getter(copy)]
    kani_witness: bool,
    #[getter(copy)]
    creusot_witness: bool,
    #[getter(copy)]
    verus_witness: bool,
    #[getter(copy)]
    proof_test: bool,
    #[getter(copy)]
    status: AmenableStdStatus,
    skip_reason: Option<String>,
    #[serde(default)]
    #[getter(copy)]
    kani_excepted: bool,
    #[serde(default)]
    #[getter(copy)]
    creusot_excepted: bool,
    #[serde(default)]
    #[getter(copy)]
    verus_excepted: bool,
}

/// Coverage report for amenable std registry vs std type inventory.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct AmenableStdReport {
    /// Crate that defined the foreign type.
    source_crate: String,
    /// Crate that provides the impl under review.
    impl_crate: String,
    /// Whether nightly-only items are in scope.
    #[getter(copy)]
    include_nightly: bool,
    /// Per-item coverage rows.
    entries: Vec<AmenableStdEntry>,
    /// How many rows are complete.
    #[getter(copy)]
    complete_count: usize,
    /// How many rows are partial.
    #[getter(copy)]
    partial_count: usize,
    /// How many items are still missing.
    #[getter(copy)]
    missing_count: usize,
    /// How many rows were skipped.
    #[getter(copy)]
    skipped_count: usize,
}

impl AmenableStdReport {
    /// Covered items as a percentage of the inventory.
    #[instrument(level = "debug", skip(self))]
    pub fn coverage_pct(&self) -> f32 {
        let accountable = self.entries.len().saturating_sub(self.skipped_count);
        if accountable == 0 {
            0.0
        } else {
            self.complete_count as f32 / accountable as f32 * 100.0
        }
    }
}

/// One actionable gap row for amenable std registry coverage.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct AmenableStdGapEntry {
    source_crate: String,
    type_path: String,
    type_kind: String,
    #[getter(copy)]
    status: AmenableStdStatus,
    missing_layers: String,
    action: String,
}
