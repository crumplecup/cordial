use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, Serialize, Deserialize, derive_new::new)]
pub struct EdgeWeight {
    pub kind: EdgeKind,
    #[new(default)]
    pub label: Option<String>,
}
