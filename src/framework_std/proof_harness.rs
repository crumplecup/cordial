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
#[instrument(level = "debug", err(level = "warn"))]
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

#[instrument(level = "debug", skip(path), err(level = "warn"))]
fn collect_proof_chain_subjects_from_file(path: &Path) -> CordialResult<HashSet<String>> {
    let source = std::fs::read_to_string(path)?;
    let mut subjects = HashSet::new();
    collect_string_args(&source, "proof_chain", &mut subjects);
    collect_string_args(&source, "proof_chain_for_verifiers", &mut subjects);
    Ok(subjects)
}

/// Finds every call to `fn_name(...)` in `source` and collects its first
/// string-literal argument, tolerating rustfmt wrapping the call across
/// multiple lines (a long subject like `"ExtStandard<jiff::SignedDurationRound>"`
/// pushes the line past the width limit, so the opening quote can land on
/// its own line rather than directly after the paren).
#[instrument(level = "debug", skip(source, subjects))]
fn collect_string_args(source: &str, fn_name: &str, subjects: &mut HashSet<String>) {
    let prefix = format!("{fn_name}(");
    let mut rest = source;
    while let Some(idx) = rest.find(&prefix) {
        rest = &rest[idx + prefix.len()..];
        if let Some(subject) = extract_leading_string_arg(rest) {
            subjects.insert(subject);
        }
    }
}

/// Reads a leading (whitespace-tolerant) string-literal argument from the
/// start of `text`, which begins immediately after a call's opening paren.
#[instrument(level = "debug")]
fn extract_leading_string_arg(text: &str) -> Option<String> {
    let quoted = text.trim_start().strip_prefix('"')?;
    let end = quoted.find('"')?;
    let subject = quoted[..end].trim();
    if subject.is_empty() {
        None
    } else {
        Some(subject.to_string())
    }
}
