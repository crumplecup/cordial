use std::fmt::{Display, Formatter, Result as FmtResult};

use serde::{Deserialize, Serialize};

use tracing::instrument;
/// Type-graph probe rule identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InternalErrorTypeProbeId {
    InternalLeaf001,
    InternalLink001,
    InternalNested001,
}

impl InternalErrorTypeProbeId {
    #[instrument(level = "trace", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InternalLeaf001 => "ERROR-CHAIN-INTERNAL-LEAF-001",
            Self::InternalLink001 => "ERROR-CHAIN-INTERNAL-LINK-001",
            Self::InternalNested001 => "ERROR-CHAIN-INTERNAL-NESTED-001",
        }
    }

    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "ERROR-CHAIN-INTERNAL-LEAF-001" => Some(Self::InternalLeaf001),
            "ERROR-CHAIN-INTERNAL-LINK-001" => Some(Self::InternalLink001),
            "ERROR-CHAIN-INTERNAL-NESTED-001" => Some(Self::InternalNested001),
            _ => None,
        }
    }
}

impl Display for InternalErrorTypeProbeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}
