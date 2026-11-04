use crate::objects::Rule;

use super::{InternalErrorComplianceId, InternalErrorTypeProbeId};

use tracing::instrument;
#[derive(Debug, Clone)]
pub struct InternalErrorChainRule {
    rule_id: String,
}

impl InternalErrorChainRule {
    #[instrument(level = "debug", skip(probe_id), ret)]
    pub fn from_probe(probe_id: InternalErrorTypeProbeId) -> Self {
        Self {
            rule_id: probe_id.as_str().to_string(),
        }
    }

    #[instrument(level = "debug", skip(compliance_id), ret)]
    pub fn from_compliance(compliance_id: InternalErrorComplianceId) -> Self {
        Self {
            rule_id: compliance_id.as_str().to_string(),
        }
    }
}

impl crate::TomlConfigured for InternalErrorChainRule {
    #[tracing::instrument(level = "trace", skip(self))]
    fn configuration_key(&self) -> &str {
        "internal_error_chain"
    }
}

impl Rule for InternalErrorChainRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        &self.rule_id
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Internal error type graph node or compliance violation"
    }
}
