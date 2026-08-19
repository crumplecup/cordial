use std::fmt::{Display, Formatter, Result as FmtResult};

use petgraph::stable_graph::NodeIndex;
use serde::{Deserialize, Serialize};

use tracing::instrument;
/// Opaque stable node identifier wrapping a petgraph index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u32);

impl NodeId {
    #[instrument(level = "debug", skip(index), ret)]
    pub(crate) fn from_index(index: NodeIndex) -> Self {
        Self(index.index() as u32)
    }

    #[instrument(level = "debug", skip(self))]
    pub(crate) fn to_index(self) -> NodeIndex {
        NodeIndex::new(self.0 as usize)
    }
}

impl Display for NodeId {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "node:{}", self.0)
    }
}

/// Kind of node in the workspace IR graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    Workspace,
    Crate,
    Module,
    Item(ItemKind),
    ImplBlock,
    ImplItem,
    Field,
    Variant,
    Param,
    Expr,
    Pat,
    Type,
    Attribute,
    Plugin(String),
}

/// Item sub-kinds shared by source and rustdoc loaders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemKind {
    Fn,
    Struct,
    Enum,
    Trait,
    TypeAlias,
    Const,
    Static,
    Macro,
    Mod,
    Other,
}

impl NodeKind {
    #[instrument(level = "trace", skip(self), ret)]
    pub fn is_item(self) -> bool {
        matches!(self, Self::Item(_))
    }
}

/// Weight stored at each graph node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeWeight {
    pub kind: NodeKind,
    pub name: Option<String>,
    pub span: Option<crate::objects::FileSpan>,
    pub attrs: Vec<(String, serde_json::Value)>,
}

impl NodeWeight {
    #[instrument(level = "debug", skip(kind), ret)]
    pub fn new(kind: NodeKind) -> Self {
        Self {
            kind,
            name: None,
            span: None,
            attrs: Vec::new(),
        }
    }

    #[instrument(level = "trace", skip(self, name))]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    #[instrument(level = "trace", skip(self, span))]
    pub fn with_span(mut self, span: crate::objects::FileSpan) -> Self {
        self.span = Some(span);
        self
    }

    #[instrument(level = "trace", skip(self, value))]
    pub fn set_attr(&mut self, key: &str, value: serde_json::Value) {
        self.attrs.push((key.to_string(), value));
    }

    #[instrument(level = "trace", skip(self))]
    pub fn attr(&self, key: &str) -> Option<&serde_json::Value> {
        self.attrs
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }
}
