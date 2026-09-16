use tracing::instrument;

use super::{AmenableStdEntry, AmenableStdGapEntry, AmenableStdReport, AmenableStdStatus};

/// Gap metadata for one amenable std row.
#[instrument(level = "debug", skip(entry))]
pub fn amenable_gap_fields(entry: &AmenableStdEntry, impl_crate: &str) -> (String, String) {
    wrapped_gap_fields(entry, impl_crate, "RustStdStandard")
}

/// Gap metadata for one amenable ext (third-party crate) row.
#[instrument(level = "debug", skip(entry))]
pub fn amenable_ext_gap_fields(entry: &AmenableStdEntry, impl_crate: &str) -> (String, String) {
    wrapped_gap_fields(entry, impl_crate, "ExtStandard")
}

/// As [`amenable_gap_fields`]/[`amenable_ext_gap_fields`], naming the
/// wrapper type explicitly -- shared by [`build_amenable_std_gaps`] and
/// [`build_amenable_ext_gaps`], which differ only in which wrapper type
/// (`RustStdStandard` or `ExtStandard`) the "register evidence" action
/// names.
#[instrument(level = "debug", skip(entry))]
fn wrapped_gap_fields(
    entry: &AmenableStdEntry,
    impl_crate: &str,
    wrapper_name: &str,
) -> (String, String) {
    (
        missing_layer_labels(entry).join(", "),
        gap_action(entry, impl_crate, wrapper_name),
    )
}

/// Build consolidated gap rows from a report, naming `wrapper_name` in
/// each "register evidence" action -- shared by
/// [`build_amenable_std_gaps`] and [`build_amenable_ext_gaps`].
#[instrument(level = "debug", skip(report))]
fn build_wrapped_gaps(report: &AmenableStdReport, wrapper_name: &str) -> Vec<AmenableStdGapEntry> {
    report
        .entries()
        .iter()
        .filter(|entry| {
            matches!(
                entry.status(),
                AmenableStdStatus::Missing | AmenableStdStatus::Partial
            )
        })
        .map(|entry| {
            let (missing_layers, action) =
                wrapped_gap_fields(entry, report.impl_crate(), wrapper_name);
            AmenableStdGapEntry::new(
                report.source_crate().clone(),
                entry.type_path().clone(),
                entry.type_kind().clone(),
                entry.status(),
                missing_layers,
                action,
            )
        })
        .collect()
}

/// Build consolidated gap rows from an amenable std report.
#[instrument(level = "debug", skip(report))]
pub fn build_amenable_std_gaps(report: &AmenableStdReport) -> Vec<AmenableStdGapEntry> {
    build_wrapped_gaps(report, "RustStdStandard")
}

/// Build consolidated gap rows from an amenable ext report.
#[instrument(level = "debug", skip(report))]
pub fn build_amenable_ext_gaps(report: &AmenableStdReport) -> Vec<AmenableStdGapEntry> {
    build_wrapped_gaps(report, "ExtStandard")
}

#[instrument(level = "debug", skip(entry))]
fn missing_layer_labels(entry: &AmenableStdEntry) -> Vec<&'static str> {
    let mut missing = Vec::new();
    if !entry.evidence_link() {
        missing.push("evidence_link");
    }
    if entry.evidence_link() && !entry.kani_witness() && !entry.kani_excepted() {
        missing.push("kani_witness");
    }
    if entry.evidence_link() && !entry.creusot_witness() && !entry.creusot_excepted() {
        missing.push("creusot_witness");
    }
    if entry.evidence_link() && !entry.verus_witness() && !entry.verus_excepted() {
        missing.push("verus_witness");
    }
    if !entry.proof_test() {
        missing.push("proof_test");
    }
    missing
}

#[instrument(level = "debug", skip(entry))]
fn gap_action(entry: &AmenableStdEntry, impl_crate: &str, wrapper_name: &str) -> String {
    if !entry.evidence_link() {
        let type_name = entry
            .type_path()
            .rsplit("::")
            .next()
            .unwrap_or(entry.type_path());
        return format!("Register `{wrapper_name}<{type_name}>` evidence in `{impl_crate}`");
    }
    let mut parts = Vec::new();
    if !entry.kani_witness() && !entry.kani_excepted() {
        parts.push("KaniWitness");
    }
    if !entry.creusot_witness() && !entry.creusot_excepted() {
        parts.push("Creusot witness");
    }
    if !entry.verus_witness() && !entry.verus_excepted() {
        parts.push("Verus witness");
    }
    if !entry.proof_test() {
        parts.push("proof_chain test in proof_chain_test.rs");
    }
    format!(
        "Add {} for `{}`",
        parts.join(" + "),
        entry
            .evidence_name()
            .as_deref()
            .unwrap_or(entry.type_path())
    )
}
