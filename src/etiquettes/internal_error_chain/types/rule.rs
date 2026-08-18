use crate::objects::Rule;

use super::{InternalErrorComplianceId, InternalErrorTypeProbeId};

#[derive(Debug, Clone)]
pub struct InternalErrorChainRule {
    pub rule_id: String,
}

impl InternalErrorChainRule {
    pub fn from_probe(probe_id: InternalErrorTypeProbeId) -> Self {
        Self {
            rule_id: probe_id.as_str().to_string(),
        }
    }

    pub fn from_compliance(compliance_id: InternalErrorComplianceId) -> Self {
        Self {
            rule_id: compliance_id.as_str().to_string(),
        }
    }
}

impl Rule for InternalErrorChainRule {
    fn id(&self) -> &str {
        &self.rule_id
    }

    fn category(&self) -> &str {
        "internal_error_chain"
    }

    fn description(&self) -> &str {
        "Internal error type graph node or compliance violation"
    }
}
