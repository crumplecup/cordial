//! Coverage entries and reports for the instantiation tests, and the lookups that read them back.
#![cfg(feature = "amenable_std")]

use std::collections::BTreeMap;

use miette::IntoDiagnostic;

use cordial::testing::{
    AmenableStdEntry, AmenableStdReport, AmenableStdStatus, ExpectedInstantiations,
};

pub const WITNESSED: [&str; 3] = ["kani", "creusot", "verus"];

pub fn parent_entry(path: &str) -> miette::Result<AmenableStdEntry> {
    AmenableStdEntry::builder()
        .type_path(path.to_string())
        .type_kind("struct".to_string())
        .is_generic(true)
        .evidence_link(true)
        .evidence_name(None)
        .kani_witness(true)
        .creusot_witness(true)
        .verus_witness(true)
        .proof_test(false)
        .status(AmenableStdStatus::Complete)
        .skip_reason(None)
        .kani_excepted(false)
        .creusot_excepted(false)
        .verus_excepted(false)
        .build()
        .into_diagnostic()
}

pub fn report(entries: Vec<AmenableStdEntry>) -> miette::Result<AmenableStdReport> {
    AmenableStdReport::builder()
        .source_crate("chrono".to_string())
        .impl_crate("amenable_ext".to_string())
        .include_nightly(false)
        .entries(entries)
        .complete_count(0)
        .partial_count(0)
        .missing_count(0)
        .skipped_count(0)
        .build()
        .into_diagnostic()
}

pub fn expected(tuples: &[&str]) -> ExpectedInstantiations {
    ExpectedInstantiations::new(BTreeMap::from([(
        "chrono::DateTime".to_string(),
        tuples.iter().map(|t| vec![(*t).to_string()]).collect(),
    )]))
}

pub fn four_zones() -> ExpectedInstantiations {
    expected(&[
        "chrono::Utc",
        "chrono::FixedOffset",
        "chrono::Local",
        "chrono_tz::Tz",
    ])
}

pub fn find<'a>(report: &'a AmenableStdReport, path: &str) -> miette::Result<&'a AmenableStdEntry> {
    report
        .entries()
        .iter()
        .find(|entry| entry.type_path() == path)
        .ok_or_else(|| miette::miette!("no row `{path}`"))
}

pub fn note(entry: &AmenableStdEntry) -> String {
    entry.note().clone().unwrap_or_default()
}
