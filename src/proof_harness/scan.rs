//! Scan proof harness test files for Kani / composition markers.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::error::CordialResult;
use crate::plugin::{WorkspaceHub, discover_workspace_hub};

/// Scanned contents of the proof harness test files.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProofHarness {
    /// Type name strings found in `assert_proofs_non_empty::<T>()` calls.
    pub non_empty_types: HashSet<String>,
    /// `(Outer, Inner)` pairs from `assert_kani_contains::<Outer, Inner>()`.
    pub composition_pairs: Vec<(String, String)>,
    /// Subject strings from `proof_chain("…")` and `proof_chain_for_verifiers("…", …)`.
    pub proof_chain_subjects: HashSet<String>,
}

impl ProofHarness {
    #[instrument(level = "debug", skip(self))]
    pub fn merge(&mut self, other: ProofHarness) {
        self.non_empty_types.extend(other.non_empty_types);
        self.composition_pairs.extend(other.composition_pairs);
        self.proof_chain_subjects.extend(other.proof_chain_subjects);
    }
}

/// Proof harness paths for a workspace hub (ported from elicit_doc).
#[instrument(level = "debug", skip(workspace))]
pub fn proof_harness_paths(hub: WorkspaceHub, workspace: &Path) -> Vec<PathBuf> {
    match hub {
        WorkspaceHub::Elicitation => vec![
            workspace.join("crates/elicitation/tests/proof_non_empty_test.rs"),
            workspace.join("crates/elicitation/tests/proof_composition_test.rs"),
        ],
        WorkspaceHub::Amenable => vec![
            workspace.join("crates/amenable/tests/proof_assessment_test.rs"),
            workspace.join("crates/amenable/tests/proof_chain_test.rs"),
        ],
        WorkspaceHub::Homecoming => vec![
            workspace.join("crates/homecoming_core/tests/code_test.rs"),
            workspace.join("crates/homecoming_core/tests/scope_test.rs"),
            workspace.join("crates/homecoming_core/tests/calculator_test.rs"),
        ],
        WorkspaceHub::Unknown => Vec::new(),
    }
}

/// Load and merge proof harness scans for the workspace at `project_root`.
#[instrument(level = "info", skip(filter), err(level = "warn"))]
pub fn load_workspace_proof_harness(
    project_root: &Path,
    filter: &dyn crate::session::RunFilter,
) -> CordialResult<ProofHarness> {
    let hub = discover_workspace_hub(project_root, filter)?;
    let mut harness = ProofHarness::default();
    for path in proof_harness_paths(hub, project_root) {
        if path.is_file() {
            harness.merge(collect_proof_harness(&path)?);
        }
    }
    Ok(harness)
}

/// Scan a proof harness test file.
#[instrument(level = "debug", skip(path), err(level = "warn"))]
pub fn collect_proof_harness(path: &Path) -> CordialResult<ProofHarness> {
    let source = std::fs::read_to_string(path)?;

    let mut non_empty_types: HashSet<String> = HashSet::new();
    let mut composition_pairs: Vec<(String, String)> = Vec::new();
    let mut proof_chain_subjects: HashSet<String> = HashSet::new();

    for line in source.lines() {
        let trimmed = line.trim();

        if let Some(ty) = extract_turbofish_arg(trimmed, "assert_proofs_non_empty") {
            non_empty_types.insert(ty);
        }

        if let Some((outer, inner)) = extract_kani_contains(trimmed) {
            composition_pairs.push((outer, inner));
        }

        if let Some(subject) = extract_string_arg(trimmed, "proof_chain") {
            proof_chain_subjects.insert(subject);
        }
        if let Some(subject) = extract_string_arg(trimmed, "proof_chain_for_verifiers") {
            proof_chain_subjects.insert(subject);
        }
    }

    Ok(ProofHarness {
        non_empty_types,
        composition_pairs,
        proof_chain_subjects,
    })
}

fn extract_turbofish_arg(line: &str, fn_name: &str) -> Option<String> {
    let prefix = format!("{fn_name}::<");
    let start = line.find(&prefix)? + prefix.len();
    let rest = &line[start..];
    let end = find_matching_angle(rest)?;
    let ty = rest[..end].trim().to_string();
    if ty.is_empty() { None } else { Some(ty) }
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

fn extract_kani_contains(line: &str) -> Option<(String, String)> {
    let prefix = "assert_kani_contains::<";
    let start = line.find(prefix)? + prefix.len();
    let rest = &line[start..];
    let end = find_matching_angle(rest)?;
    let inner_str = &rest[..end];
    let comma = find_top_level_comma(inner_str)?;
    let outer = inner_str[..comma].trim().to_string();
    let inner = inner_str[comma + 1..].trim().to_string();
    if outer.is_empty() || inner.is_empty() {
        None
    } else {
        Some((outer, inner))
    }
}

fn find_matching_angle(s: &str) -> Option<usize> {
    let mut depth: i32 = 1;
    for (i, ch) in s.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_top_level_comma(s: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}
