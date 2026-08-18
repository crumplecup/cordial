use std::fmt::{Display, Formatter, Result as FmtResult};

use serde::{Deserialize, Serialize};

/// Non-compliant error-handling pattern at a call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InternalErrorComplianceId {
    StringifyForeign001,
    DiscardTyped001,
}

impl InternalErrorComplianceId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StringifyForeign001 => "ERROR-CHAIN-COMPLIANCE-STRINGIFY-001",
            Self::DiscardTyped001 => "ERROR-CHAIN-COMPLIANCE-DISCARD-TYPED-001",
        }
    }

    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "ERROR-CHAIN-COMPLIANCE-STRINGIFY-001" => Some(Self::StringifyForeign001),
            "ERROR-CHAIN-COMPLIANCE-DISCARD-TYPED-001" => Some(Self::DiscardTyped001),
            _ => None,
        }
    }
}

impl Display for InternalErrorComplianceId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}
