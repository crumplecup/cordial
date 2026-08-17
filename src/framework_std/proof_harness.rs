//! Scan amenable proof harness tests for `proof_chain` subjects.

use std::collections::HashSet;
use std::path::Path;

use tracing::instrument;

use crate::error::CordialResult;

/// Default proof harness paths relative to an amenable workspace root.
pub const AMENABLE_PROOF_HARNESS_PATHS: &[&str] = &[
    "crates/amenable/tests/proof_assessment_test.rs",
    "crates/amenable/tests/proof_chain_test.rs",
];

/// Scan proof harness files and collect `proof_chain` / `proof_chain_for_verifiers` subjects.
#[instrument(skip(project_root))]
pub fn collect_proof_chain_subjects(project_root: &Path) -> CordialResult<HashSet<String>> {
    let mut subjects = HashSet::new();
    for relative in AMENABLE_PROOF_HARNESS_PATHS {
        let path = project_root.join(relative);
        if path.is_file() {
            subjects.extend(collect_proof_chain_subjects_from_file(&path)?);
        }
    }
    Ok(subjects)
}

fn collect_proof_chain_subjects_from_file(path: &Path) -> CordialResult<HashSet<String>> {
    let source = std::fs::read_to_string(path)?;
    let mut subjects = HashSet::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(subject) = extract_string_arg(trimmed, "proof_chain") {
            subjects.insert(subject);
        }
        if let Some(subject) = extract_string_arg(trimmed, "proof_chain_for_verifiers") {
            subjects.insert(subject);
        }
    }
    Ok(subjects)
}

fn extract_string_arg(line: &str, fn_name: &str) -> Option<String> {
    let prefix = format!("{fn_name}(\"");
    let start = line.find(&prefix)? + prefix.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    let subject = rest[..end].trim();
    if subject.is_empty() {
        None
    } else {
        Some(subject.to_string())
    }
}
