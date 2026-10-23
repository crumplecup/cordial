use super::{InternalErrorComplianceReport, InternalErrorTypeGraphReport};
use crate::etiquettes::internal_error_chain::SignatureChainCheck;

/// Combined internal error-chain scan for one crate.
#[derive(Debug, Clone, PartialEq, Eq, Default, derive_new::new, derive_getters::Getters)]
pub struct InternalErrorChainScanReport {
    /// Cargo package name.
    crate_name: String,
    /// Type-relationship graph for this crate.
    type_graph: InternalErrorTypeGraphReport,
    /// Compliance findings for this crate.
    compliance: InternalErrorComplianceReport,
    /// Resolved, broken, and unknown signature-level conversion checks.
    signature_checks: Vec<SignatureChainCheck>,
}
