//! Proof-chain subjects from the shared proof-harness scanner.

use std::collections::HashSet;
use std::path::Path;

use crate::error::CordialResult;
use crate::loader::SourceLoadView;
use crate::plugin::WorkspaceHub;
use crate::proof_harness::{collect_proof_harness_with_source, proof_harness_paths};

use tracing::instrument;

/// Collect proof-chain subjects through the same parsed-source path as the IR.
#[instrument(level = "debug", err(level = "warn"))]
pub fn collect_proof_chain_subjects(project_root: &Path) -> CordialResult<HashSet<String>> {
    let source = SourceLoadView::for_standalone_scan();
    let mut subjects = HashSet::new();
    for path in proof_harness_paths(WorkspaceHub::Amenable, project_root) {
        if path.is_file() {
            subjects.extend(
                collect_proof_harness_with_source(&path, &source)?
                    .proof_chain_subjects()
                    .iter()
                    .cloned(),
            );
        }
    }
    Ok(subjects)
}
