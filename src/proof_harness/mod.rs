//! Proof harness scan and type-path test status for impl coverage.

mod scan;
#[cfg(feature = "impl_coverage")]
mod test_status;

pub use scan::ProofHarness;
#[cfg(feature = "impl_coverage")]
pub use scan::{collect_proof_harness, load_workspace_proof_harness};
pub(crate) use scan::{
    collect_proof_harness_with_source, load_workspace_proof_harness_with_source,
    proof_harness_paths,
};
#[cfg(feature = "impl_coverage")]
pub use test_status::{TestStatus, test_status_for_type_path};
