use super::{InternalErrorComplianceFinding, InternalErrorComplianceId};

/// Compliance scan output for one crate.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InternalErrorComplianceReport {
    pub crate_name: String,
    pub findings: Vec<InternalErrorComplianceFinding>,
}

impl InternalErrorComplianceReport {
    pub fn stringify_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.rule_id == InternalErrorComplianceId::StringifyForeign001)
            .count()
    }

    pub fn discard_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.rule_id == InternalErrorComplianceId::DiscardTyped001)
            .count()
    }
}
