use std::fmt::{Display, Formatter, Result as FmtResult};

use petgraph::stable_graph::NodeIndex;
use serde::{Deserialize, Serialize};

use tracing::instrument;

/// Node-level read API used by queries and probes.
pub trait NodeView {
    /// Crate-local node id.
    fn id(&self) -> NodeId;
    /// Node kind.
    fn kind(&self) -> &NodeKind;
    /// Latest attribute value stored under `key`, if present.
    fn attr(&self, key: &str) -> Option<&serde_json::Value>;
}

/// Opaque stable node identifier wrapping a petgraph index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(u32);

impl NodeId {
    /// Build a node id from its stable numeric value.
    #[instrument(level = "debug", ret)]
    #[hotpath::measure]
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    /// Stable numeric value for serialization-adjacent attributes.
    #[instrument(level = "trace", skip(self))]
    pub fn value(self) -> u32 {
        self.0
    }

    #[instrument(level = "debug", skip(index), ret)]
    #[hotpath::measure]
    pub(crate) fn from_index(index: NodeIndex) -> Self {
        Self(index.index() as u32)
    }

    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure]
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
    PartialEq,
    Eq,
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
    #[getter(skip)]
    #[serde(with = "attrs_wire")]
    attrs: Vec<(String, Attr)>,
}

/// The attributes that are bulk: a function's semantic facts, of which a graph holds one or two for
/// every function. They are kept encoded (a fraction of the memory of the JSON value) and decoded when
/// something reads them; [`NodeWeight::drop_decoded`] puts them back to bytes.
const PACKED_KEYS: [&str; 2] = [
    super::semantic::ATTR_SEMANTIC_FUNCTION,
    super::semantic_context::ATTR_SEMANTIC_CONTEXT_FUNCTIONS,
];

/// One attribute: a JSON value, or, for a bulk key, its encoding and the value decoded from it.
#[derive(Clone)]
enum Attr {
    Value(serde_json::Value),
    Packed(Packed),
}

#[derive(Clone)]
struct Packed {
    bytes: Box<[u8]>,
    decoded: std::sync::OnceLock<serde_json::Value>,
}

/// An attribute prints as its value, not as how it is held: whether it was decoded yet is not
/// something that distinguishes two graphs.
impl std::fmt::Debug for Attr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.value(), f)
    }
}

impl PartialEq for Attr {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Value(left), Self::Value(right)) => left == right,
            (Self::Packed(left), Self::Packed(right)) => left.bytes == right.bytes,
            (Self::Value(value), Self::Packed(packed))
            | (Self::Packed(packed), Self::Value(value)) => {
                rmp_serde::to_vec(value).is_ok_and(|bytes| *bytes == *packed.bytes)
            }
        }
    }
}

impl Eq for Attr {}

impl Attr {
    /// `value` for `key`, packed when the key is a bulk one and the value encodes.
    fn of(key: &str, value: serde_json::Value) -> Self {
        if PACKED_KEYS.contains(&key)
            && let Ok(bytes) = rmp_serde::to_vec(&value)
        {
            return Self::Packed(Packed {
                bytes: bytes.into_boxed_slice(),
                decoded: std::sync::OnceLock::new(),
            });
        }
        Self::Value(value)
    }

    /// The value: as it is, or decoded (once) from its bytes.
    fn value(&self) -> Option<&serde_json::Value> {
        match self {
            Self::Value(value) => Some(value),
            Self::Packed(packed) => {
                if packed.decoded.get().is_none() {
                    let value = rmp_serde::from_slice(&packed.bytes).ok()?;
                    let _ = packed.decoded.set(value);
                }
                packed.decoded.get()
            }
        }
    }
}

/// How attributes are written and read: as the JSON values they always were, whatever they are held
/// as, so that what is saved and exported does not change.
mod attrs_wire {
    use super::Attr;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(super) fn serialize<S: Serializer>(
        attrs: &[(String, Attr)],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let values: Vec<(&String, &serde_json::Value)> = attrs
            .iter()
            .filter_map(|(key, attr)| attr.value().map(|value| (key, value)))
            .collect();
        values.serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<(String, Attr)>, D::Error> {
        let values = Vec::<(String, serde_json::Value)>::deserialize(deserializer)?;
        Ok(values
            .into_iter()
            .map(|(key, value)| {
                let attr = Attr::of(&key, value);
                (key, attr)
            })
            .collect())
    }
}

impl NodeWeight {
    /// Append a JSON attribute to this node.
    ///
    /// Attribute lookup returns the latest value for a key, so repeated keys
    /// are allowed when later enrichers refine earlier loader facts.
    #[instrument(level = "trace", skip(self, value))]
    pub fn set_attr(&mut self, key: &str, value: serde_json::Value) {
        let attr = Attr::of(key, value);
        self.attrs.push((key.to_string(), attr));
    }

    /// Latest attribute value stored under `key`.
    #[instrument(level = "trace", skip(self))]
    pub fn attr(&self, key: &str) -> Option<&serde_json::Value> {
        self.attrs
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .and_then(|(_, attr)| attr.value())
    }

    /// Every attribute as the JSON value it is, in the order they were set (a copy: bulk attributes
    /// are held encoded).
    #[instrument(level = "trace", skip(self))]
    pub fn attrs(&self) -> Vec<(String, serde_json::Value)> {
        self.attrs
            .iter()
            .filter_map(|(key, attr)| attr.value().map(|value| (key.clone(), value.clone())))
            .collect()
    }

    /// Put the bulk attributes that were read back to the bytes they are held as, so that what a
    /// stage read does not stay in memory as JSON until the node is dropped. Returns how many.
    #[instrument(level = "trace", skip(self))]
    pub fn drop_decoded(&mut self) -> usize {
        let mut dropped = 0;
        for (_, attr) in &mut self.attrs {
            if let Attr::Packed(packed) = attr
                && packed.decoded.take().is_some()
            {
                dropped += 1;
            }
        }
        dropped
    }

    /// How many of the node's bulk attributes are held as bytes only, and how many of them as a
    /// decoded value as well: for the tests of what packing does.
    #[instrument(level = "trace", skip(self))]
    pub fn packed(&self) -> (usize, usize) {
        let (mut packed, mut decoded) = (0, 0);
        for (_, attr) in &self.attrs {
            if let Attr::Packed(held) = attr {
                packed += 1;
                decoded += usize::from(held.decoded.get().is_some());
            }
        }
        (packed, decoded)
    }
}

/// Borrowed node handle returned by [`super::IrView`] queries.
#[derive(Debug, Clone, Copy, derive_getters::Getters, derive_new::new)]
pub struct NodeRef<'a> {
    /// Crate-local node id.
    #[getter(copy)]
    id: NodeId,
    /// Borrowed node payload.
    weight: &'a NodeWeight,
}

impl<'a> NodeRef<'a> {
    /// Node kind.
    #[instrument(level = "trace", skip(self))]
    pub fn kind(&self) -> &NodeKind {
        self.weight.kind()
    }

    /// Latest attribute value stored under `key`, if present.
    #[instrument(level = "trace", skip(self))]
    pub fn attr(&self, key: &str) -> Option<&'a serde_json::Value> {
        self.weight.attr(key)
    }
}

impl NodeView for NodeRef<'_> {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> NodeId {
        self.id
    }

    #[instrument(level = "trace", skip(self))]
    fn kind(&self) -> &NodeKind {
        self.weight.kind()
    }

    #[instrument(level = "trace", skip(self))]
    fn attr(&self, key: &str) -> Option<&serde_json::Value> {
        self.weight.attr(key)
    }
}
