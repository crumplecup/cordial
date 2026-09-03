use super::{InternalErrorComplianceReport, InternalErrorTypeGraphReport};

/// Combined internal error-chain scan for one crate.
#[derive(Debug, Clone, PartialEq, Eq, Default, derive_new::new, derive_getters::Getters)]
pub struct InternalErrorChainScanReport {
    /// Cargo package name.
    crate_name: String,
    /// Type-relationship graph for this crate.
    type_graph: InternalErrorTypeGraphReport,
    /// Compliance findings for this crate.
    compliance: InternalErrorComplianceReport,
}
