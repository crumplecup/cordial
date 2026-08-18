use serde::{Deserialize, Serialize};

use tracing::instrument;
/// Kind of directed edge in the workspace IR graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    Contains,
    Defines,
    Scope,
    Implements,
    Aliases,
    Reexports,
    Wraps,
    Mirrors,
    Depends,
    ErrorFlow,
    HasAttr,
    Plugin,
}

/// Weight stored at each graph edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeWeight {
    pub kind: EdgeKind,
    pub label: Option<String>,
}

impl EdgeWeight {
    #[instrument(level = "debug", skip(kind), ret)]
    pub fn new(kind: EdgeKind) -> Self {
        Self { kind, label: None }
    }
}
