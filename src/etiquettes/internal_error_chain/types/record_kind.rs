/// Distinguishes type-graph inventory rows from compliance violations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternalErrorRecordKind {
    TypeGraph,
    Compliance,
}

impl InternalErrorRecordKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TypeGraph => "type_graph",
            Self::Compliance => "compliance",
        }
    }

    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "type_graph" => Some(Self::TypeGraph),
            "compliance" => Some(Self::Compliance),
            _ => None,
        }
    }
}
