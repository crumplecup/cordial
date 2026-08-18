use super::{InternalErrorComplianceReport, InternalErrorTypeGraphReport};

/// Combined internal error-chain scan for one crate.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InternalErrorChainScanReport {
    pub crate_name: String,
    pub type_graph: InternalErrorTypeGraphReport,
    pub compliance: InternalErrorComplianceReport,
}
