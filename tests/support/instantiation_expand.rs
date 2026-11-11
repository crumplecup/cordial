//! Expanding a one-parent report against a registry of claims.
#![cfg(feature = "amenable_std")]

use std::collections::HashSet;

use miette::IntoDiagnostic;
use serde_json::{Value, json};

use cordial::testing::{
    AmenableRegistryEvidence, AmenableStdReport, ExpectedInstantiations, InstantiationContext,
    RegistryDump, VerifierSkipMap, expand_report,
};

use crate::instantiation_reports::{WITNESSED, parent_entry, report};
use crate::instantiation_world::resolver;

/// A registry dump with these concrete claims (each with the verifiers that
/// have a witness for it) plus the generic `DateTime<Tz>` claim.
pub fn registry(claims: &[(&str, &[&str])]) -> miette::Result<RegistryDump> {
    let links: Vec<Value> = claims
        .iter()
        .map(|(ty, _)| {
            json!({"name": format!("amenable_ext::ExtStandard<{ty}>"), "basis": "", "index": 0})
        })
        .collect();
    let proofs: Vec<Value> = claims
        .iter()
        .flat_map(|(ty, verifiers)| {
            verifiers.iter().map(move |verifier| {
                json!({"evidence": format!("amenable_ext::ExtStandard<{ty}>"), "verifier": verifier})
            })
        })
        .collect();
    serde_json::from_value(json!({
        "evidence_links": links, "proof_records": proofs, "kani_proofs": [],
    }))
    .into_diagnostic()
}

/// Expand a one-parent report and return its rows.
pub fn expand(
    claims: &[(&str, &[&str])],
    expected: &ExpectedInstantiations,
    skip_map: &VerifierSkipMap,
    proof_subjects: &HashSet<String>,
) -> miette::Result<AmenableStdReport> {
    let resolver = resolver()?;
    let dump = registry(claims)?;
    let facts = AmenableRegistryEvidence::resolve(&dump, proof_subjects, &resolver);
    let ctx = InstantiationContext::new(&resolver, &facts, skip_map, expected);
    expand_report(&report(vec![parent_entry("chrono::DateTime")?])?, &ctx).into_diagnostic()
}

pub fn all_four() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
        ("chrono::DateTime<chrono::Utc>", &WITNESSED),
        ("chrono::DateTime<chrono::FixedOffset>", &WITNESSED),
        ("chrono::DateTime<chrono::Local>", &WITNESSED),
        ("chrono::DateTime<chrono_tz::Tz>", &WITNESSED),
    ]
}
