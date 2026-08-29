use std::fmt::{Display, Formatter, Result as FmtResult};

use serde::{Deserialize, Serialize};

use tracing::instrument;
/// Non-compliant error-handling pattern (call site or source-wrapper shape).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InternalErrorComplianceId {
    /// `ERROR-CHAIN-COMPLIANCE-STRINGIFY-001`.
    StringifyForeign001,
    /// `ERROR-CHAIN-COMPLIANCE-DISCARD-TYPED-001`.
    DiscardTyped001,
    /// Foreign-source wrapper missing `source` and/or owned `file`+`line`.
    SourceShape001,
    /// Native source missing `#[track_caller] fn new` that calls `Location::caller()`, or a wrapper that would hide the call site.
    SourceTrackCaller001,
    /// Missing parent error that boxes a `*Kind` enum.
    ArchParent001,
    /// `kind` field is not `Box<Kind>`.
    ArchKindBox001,
    /// Kind variant does not hold a native source type.
    ArchKindVariant001,
    /// Native source is not a variant of any Kind.
    ArchOrphanSource001,
}

impl InternalErrorComplianceId {
    /// Stable string form of this value.
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StringifyForeign001 => "ERROR-CHAIN-COMPLIANCE-STRINGIFY-001",
            Self::DiscardTyped001 => "ERROR-CHAIN-COMPLIANCE-DISCARD-TYPED-001",
            Self::SourceShape001 => "ERROR-CHAIN-COMPLIANCE-SOURCE-SHAPE-001",
            Self::SourceTrackCaller001 => "ERROR-CHAIN-COMPLIANCE-SOURCE-TRACK-CALLER-001",
            Self::ArchParent001 => "ERROR-CHAIN-COMPLIANCE-ARCH-PARENT-001",
            Self::ArchKindBox001 => "ERROR-CHAIN-COMPLIANCE-ARCH-KIND-BOX-001",
            Self::ArchKindVariant001 => "ERROR-CHAIN-COMPLIANCE-ARCH-KIND-VARIANT-001",
            Self::ArchOrphanSource001 => "ERROR-CHAIN-COMPLIANCE-ARCH-ORPHAN-SOURCE-001",
        }
    }

    /// Parse from the stable identifier string.
    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "ERROR-CHAIN-COMPLIANCE-STRINGIFY-001" => Some(Self::StringifyForeign001),
            "ERROR-CHAIN-COMPLIANCE-DISCARD-TYPED-001" => Some(Self::DiscardTyped001),
            "ERROR-CHAIN-COMPLIANCE-SOURCE-SHAPE-001" => Some(Self::SourceShape001),
            "ERROR-CHAIN-COMPLIANCE-SOURCE-TRACK-CALLER-001" => Some(Self::SourceTrackCaller001),
            "ERROR-CHAIN-COMPLIANCE-ARCH-PARENT-001" => Some(Self::ArchParent001),
            "ERROR-CHAIN-COMPLIANCE-ARCH-KIND-BOX-001" => Some(Self::ArchKindBox001),
            "ERROR-CHAIN-COMPLIANCE-ARCH-KIND-VARIANT-001" => Some(Self::ArchKindVariant001),
            "ERROR-CHAIN-COMPLIANCE-ARCH-ORPHAN-SOURCE-001" => Some(Self::ArchOrphanSource001),
            _ => None,
        }
    }
}

impl Display for InternalErrorComplianceId {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}
