//! Proof harness scan and type-path test status for impl coverage.

mod scan;
mod test_status;

pub use scan::{ProofHarness, collect_proof_harness, load_workspace_proof_harness};
pub use test_status::{TestStatus, test_status_for_type_path};
