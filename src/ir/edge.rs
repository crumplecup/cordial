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
    /// Resolved direct source call; absence does not prove caller closure.
    Calls,
    /// Resolved direct source call in a non-ordinary cfg context, whose weight
    /// names the context. `Calls` edges stay ordinary-context only, so a
    /// reader never sees gated callers unless it asks for their context.
    ContextCalls,
    /// Node carries this attribute.
    HasAttr,
    /// Edge or node owned by a plugin.
    Plugin,
}

/// Weight stored at each graph edge.
#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    derive_new::new,
    derive_getters::Getters,
    derive_setters::Setters,
)]
#[setters(prefix = "with_", strip_option)]
pub struct EdgeWeight {
    /// Kind of this edge.
    #[getter(copy)]
    #[setters(skip)]
    kind: EdgeKind,
    /// Optional edge label.
    #[new(default)]
    #[setters(skip)]
    label: Option<String>,
    /// Cfg context that resolved this edge; set on `ContextCalls` edges only.
    #[new(default)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    context: Option<super::SemanticContextKey>,
}
