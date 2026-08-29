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
    /// The workspace root node.
    Workspace,
    /// A crate root node.
    Crate,
    /// A module node.
    Module,
    /// Item.
    Item(ItemKind),
    /// An `impl` block.
    ImplBlock,
    /// An item inside an `impl`.
    ImplItem,
    /// A struct or enum field.
    Field,
    /// An enum variant.
    Variant,
    /// A function or method parameter.
    Param,
    /// An expression node.
    Expr,
    /// A pattern node.
    Pat,
    /// A type node.
    Type,
    /// An attribute node.
    Attribute,
    /// Edge or node owned by a plugin.
    Plugin(String),
}

/// Item sub-kinds shared by source and rustdoc loaders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemKind {
    /// A function or method.
    Fn,
    /// A struct.
    Struct,
    /// An enum.
    Enum,
    /// A trait.
    Trait,
    /// A type alias.
    TypeAlias,
    /// A `const` item.
    Const,
    /// A `static` item.
    Static,
    /// A macro.
    Macro,
    /// A module item.
    Mod,
    /// Any other item kind.
    Other,
}

impl NodeKind {
    /// Whether this node is an item (fn, type, trait, …).
    #[instrument(level = "trace", skip(self), ret)]
    pub fn is_item(self) -> bool {
        matches!(self, Self::Item(_))
    }
}

/// Weight stored at each graph node.
#[derive(Debug, Clone, Serialize, Deserialize, derive_new::new)]
pub struct NodeWeight {
    /// Kind of this node.
    pub kind: NodeKind,
    /// Optional item name.
    #[new(default)]
    pub name: Option<String>,
    /// Optional source span.
    #[new(default)]
    pub span: Option<crate::objects::FileSpan>,
    /// JSON attributes attached to this node.
    #[new(default)]
    pub attrs: Vec<(String, serde_json::Value)>,
}

impl NodeWeight {
    /// Return a copy with `name` set.
    #[instrument(level = "trace", skip(self, name))]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Return a copy with `span` set.
    #[instrument(level = "trace", skip(self, span))]
    pub fn with_span(mut self, span: crate::objects::FileSpan) -> Self {
        self.span = Some(span);
        self
    }

    /// Set a JSON attribute on a node.
    #[instrument(level = "trace", skip(self, value))]
    pub fn set_attr(&mut self, key: &str, value: serde_json::Value) {
        self.attrs.push((key.to_string(), value));
    }

    /// Latest attribute value stored under `key`.
    #[instrument(level = "trace", skip(self))]
    pub fn attr(&self, key: &str) -> Option<&serde_json::Value> {
        self.attrs
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }
}
