use tracing::instrument;
/// Distinguishes type-graph inventory rows from compliance violations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternalErrorRecordKind {
    TypeGraph,
    Compliance,
}

impl InternalErrorRecordKind {
    #[instrument(level = "trace", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TypeGraph => "type_graph",
            Self::Compliance => "compliance",
        }
    }

    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "type_graph" => Some(Self::TypeGraph),
            "compliance" => Some(Self::Compliance),
            _ => None,
        }
    }
}
