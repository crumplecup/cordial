use std::fmt::{Display, Formatter, Result as FmtResult};

use serde::{Deserialize, Serialize};

use tracing::instrument;
/// Classification of one node in the crate error type graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InternalErrorNodeClass {
    InternalLeaf,
    InternalLink,
    ForeignBridge,
    UmbrellaWrapper,
}

impl InternalErrorNodeClass {
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InternalLeaf => "ERROR-CHAIN-INTERNAL-LEAF",
            Self::InternalLink => "ERROR-CHAIN-INTERNAL-LINK",
            Self::ForeignBridge => "ERROR-CHAIN-FOREIGN-BRIDGE",
            Self::UmbrellaWrapper => "ERROR-CHAIN-INTERNAL-UMBRELLA",
        }
    }

    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "ERROR-CHAIN-INTERNAL-LEAF" => Some(Self::InternalLeaf),
            "ERROR-CHAIN-INTERNAL-LINK" => Some(Self::InternalLink),
            "ERROR-CHAIN-FOREIGN-BRIDGE" => Some(Self::ForeignBridge),
            "ERROR-CHAIN-INTERNAL-UMBRELLA" => Some(Self::UmbrellaWrapper),
            _ => None,
        }
    }
}

impl Display for InternalErrorNodeClass {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}
