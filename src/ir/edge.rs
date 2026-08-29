use serde::{Deserialize, Serialize};

/// Kind of directed edge in the workspace IR graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    /// Parent lexically contains child.
    Contains,
    /// Parent defines child.
    Defines,
    /// Child is in the parent's scope.
    Scope,
    /// Type implements a trait.
    Implements,
    /// Type alias or use-rename edge.
    Aliases,
    /// Re-export edge.
    Reexports,
    /// Wrapper type around an inner type.
    Wraps,
    /// Shadow type mirroring an upstream type.
    Mirrors,
    /// Crate or item dependency.
    Depends,
    /// Error value flows from origin to site.
    ErrorFlow,
    /// Node carries this attribute.
    HasAttr,
    /// Edge or node owned by a plugin.
    Plugin,
}

/// Weight stored at each graph edge.
#[derive(Debug, Clone, Serialize, Deserialize, derive_new::new)]
pub struct EdgeWeight {
    /// Kind of this edge.
    pub kind: EdgeKind,
    /// Optional edge label.
    #[new(default)]
    pub label: Option<String>,
}
