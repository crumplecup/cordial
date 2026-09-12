use std::fmt::{Display, Formatter, Result as FmtResult};

use petgraph::stable_graph::NodeIndex;
use serde::{Deserialize, Serialize};

use tracing::instrument;
/// Opaque stable node identifier wrapping a petgraph index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(u32);

impl NodeId {
    /// Build a node id from its stable numeric value.
    #[instrument(level = "debug", ret)]
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    /// Stable numeric value for serialization-adjacent attributes.
    #[instrument(level = "trace", skip(self))]
    pub fn value(self) -> u32 {
        self.0
    }

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
#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    derive_getters::Getters,
    derive_new::new,
    derive_setters::Setters,
)]
#[setters(generate = false, prefix = "with_")]
pub struct NodeWeight {
    /// Kind of this node.
    kind: NodeKind,
    /// Optional item name.
    #[new(default)]
    #[setters(generate, strip_option, into)]
    name: Option<String>,
    /// Optional source span.
    #[new(default)]
    #[setters(generate, strip_option)]
    span: Option<crate::objects::FileSpan>,
    /// JSON attributes attached to this node.
    #[new(default)]
    attrs: Vec<(String, serde_json::Value)>,
}

impl NodeWeight {
    /// Append a JSON attribute to this node.
    ///
    /// Attribute lookup returns the latest value for a key, so repeated keys
    /// are allowed when later enrichers refine earlier loader facts.
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
