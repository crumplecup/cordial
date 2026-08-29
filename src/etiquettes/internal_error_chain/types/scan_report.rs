use super::{InternalErrorComplianceReport, InternalErrorTypeGraphReport};

/// Combined internal error-chain scan for one crate.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InternalErrorChainScanReport {
    /// Cargo package name.
    pub crate_name: String,
    /// Type-relationship graph for this crate.
    pub type_graph: InternalErrorTypeGraphReport,
    /// Compliance findings for this crate.
    pub compliance: InternalErrorComplianceReport,
}
