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
    /// Whether `key` is present, without decoding its value when supported.
    fn has_attr(&self, key: &str) -> bool {
        self.attr(key).is_some()
    }
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

/// Per-function semantic evidence kept encoded until a consumer reads it.
/// Whole-crate policy inventories also use this storage when their features are enabled.
/// [`NodeWeight::drop_decoded`] releases expanded JSON and shared policy inventories.
/// Cloned nodes share the immutable encoding and start with independent, empty decoded caches.
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

struct Packed {
    bytes: std::sync::Arc<[u8]>,
    decoded: std::sync::OnceLock<serde_json::Value>,
    #[cfg(any(
        feature = "allows",
        feature = "cfg_hygiene",
        feature = "cfg_scatter",
        feature = "glob_imports",
        feature = "inline_tests",
        feature = "modularity",
        feature = "pageantry",
        feature = "derives",
        feature = "visibility",
        feature = "cli_layout"
    ))]
    policy: Option<Box<super::source_policy::Cache>>,
}

impl Packed {
    fn new(bytes: Vec<u8>, _key: &str) -> Self {
        Self {
            bytes: bytes.into(),
            decoded: std::sync::OnceLock::new(),
            #[cfg(any(
                feature = "allows",
                feature = "cfg_hygiene",
                feature = "cfg_scatter",
                feature = "glob_imports",
                feature = "inline_tests",
                feature = "modularity",
                feature = "pageantry",
                feature = "derives",
                feature = "visibility",
                feature = "cli_layout"
            ))]
            policy: (_key == super::source_policy::ATTR_SOURCE_POLICY_INVENTORY).then(Box::default),
        }
    }
}

impl Clone for Packed {
    fn clone(&self) -> Self {
        Self {
            bytes: std::sync::Arc::clone(&self.bytes),
            decoded: std::sync::OnceLock::new(),
            #[cfg(any(
                feature = "allows",
                feature = "cfg_hygiene",
                feature = "cfg_scatter",
                feature = "glob_imports",
                feature = "inline_tests",
                feature = "modularity",
                feature = "pageantry",
                feature = "derives",
                feature = "visibility",
                feature = "cli_layout"
            ))]
            policy: self.policy.clone(),
        }
    }
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
        let bulk = PACKED_KEYS.contains(&key);
        #[cfg(any(
            feature = "allows",
            feature = "cfg_hygiene",
            feature = "cfg_scatter",
            feature = "glob_imports",
            feature = "inline_tests",
            feature = "modularity",
            feature = "pageantry",
            feature = "derives",
            feature = "visibility",
            feature = "cli_layout"
        ))]
        let bulk = bulk || key == super::source_policy::ATTR_SOURCE_POLICY_INVENTORY;
        if bulk && let Ok(bytes) = rmp_serde::to_vec(&value) {
            return Self::Packed(Packed::new(bytes, key));
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

    /// Read typed evidence directly from a cold encoding. The fallback keeps
    /// the JSON reader's validation errors and serde representation behavior.
    fn read<T: serde::de::DeserializeOwned>(&self) -> Option<serde_json::Result<T>> {
        if let Self::Packed(packed) = self
            && packed.decoded.get().is_none()
            && let Ok(value) = rmp_serde::from_slice(&packed.bytes)
        {
            return Some(Ok(value));
        }
        self.value().map(T::deserialize)
    }

    fn field(&self, field: &str) -> Option<serde_json::Value> {
        if let Self::Packed(packed) = self
            && packed.decoded.get().is_none()
            && let Ok(value) = super::attribute_field::read(&packed.bytes, field)
        {
            return value;
        }
        self.value()?.get(field).cloned()
    }
}

/// How attributes are written and read: as the JSON values they always were, whatever they are held
/// as, so that what is saved and exported does not change.
mod attrs_wire {
    use super::Attr;
    use serde::ser::SerializeSeq;
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(
        attrs: &[(String, Attr)],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        // Decode a cold packed attribute only for this element. Serialization must
        // not populate the live graph's decoded caches or retain a whole graph of JSON.
        let mut values = serializer.serialize_seq(Some(attrs.len()))?;
        for (key, attr) in attrs {
            match attr {
                Attr::Value(value) => values.serialize_element(&(key, value))?,
                Attr::Packed(packed) => {
                    if let Some(value) = packed.decoded.get() {
                        values.serialize_element(&(key, value))?;
                    } else {
                        let value: serde_json::Value = rmp_serde::from_slice(&packed.bytes)
                            .map_err(serde::ser::Error::custom)?;
                        values.serialize_element(&(key, value))?;
                    }
                }
            }
        }
        values.end()
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

    /// Put `value` where the latest attribute under `key` is, so that the attribute is as it would
    /// have been had it been set so (and not followed by a second one, which a graph that is made
    /// over for another process would carry for the rest of its life). Appends it if there is none.
    #[instrument(level = "trace", skip(self, value))]
    pub fn replace_attr(&mut self, key: &str, value: serde_json::Value) {
        match self.attrs.iter().rposition(|(k, _)| k == key) {
            Some(index) => self.attrs[index].1 = Attr::of(key, value),
            None => self.set_attr(key, value),
        }
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

    /// Copy one object field from the latest attribute under `key`.
    /// Cold packed objects skip unrelated values and leave the JSON cache cold.
    /// Missing fields and non-object attributes return `None`, as with JSON `get`.
    #[instrument(level = "trace", skip(self))]
    #[hotpath::measure]
    pub fn attr_field(&self, key: &str, field: &str) -> Option<serde_json::Value> {
        self.attrs
            .iter()
            .rev()
            .find(|(name, _)| name == key)
            .and_then(|(_, attr)| attr.field(field))
    }

    /// Whether a key is present, including JSON null, without decoding it.
    #[instrument(level = "trace", skip(self))]
    pub fn has_attr(&self, key: &str) -> bool {
        self.attrs.iter().any(|(name, _)| name == key)
    }

    /// Read the latest typed evidence directly from its encoding when possible.
    /// Successful packed reads leave the JSON cache cold; incompatible reads
    /// use the existing JSON validation path.
    #[instrument(level = "trace", skip(self))]
    #[hotpath::measure]
    pub(crate) fn read_attr<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> serde_json::Result<Option<T>> {
        self.attrs
            .iter()
            .rev()
            .find(|(name, _)| name == key)
            .and_then(|(_, attr)| attr.read())
            .transpose()
    }

    /// Share a fully validated policy inventory until decoded attributes are released.
    #[cfg(any(
        feature = "allows",
        feature = "cfg_hygiene",
        feature = "cfg_scatter",
        feature = "glob_imports",
        feature = "inline_tests",
        feature = "modularity",
        feature = "pageantry",
        feature = "derives",
        feature = "visibility",
        feature = "cli_layout"
    ))]
    #[instrument(level = "trace", skip(self))]
    pub(crate) fn shared_policy_inventory(
        &self,
    ) -> serde_json::Result<Option<std::sync::Arc<super::SourcePolicyInventory>>> {
        let Some((_, attr)) = self
            .attrs
            .iter()
            .rev()
            .find(|(name, _)| name == super::ATTR_SOURCE_POLICY_INVENTORY)
        else {
            return Ok(None);
        };
        let cache = match attr {
            Attr::Packed(packed) => packed.policy.as_deref(),
            Attr::Value(_) => None,
        };
        if let Some(inventory) = cache.and_then(super::source_policy::Cache::get) {
            return Ok(Some(inventory));
        }
        Ok(attr.read().transpose()?.map(|inventory| match cache {
            Some(cache) => cache.insert(inventory),
            None => std::sync::Arc::new(inventory),
        }))
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
    /// stage read does not stay in memory until the node is dropped. Releases shared policy
    /// inventory caches as well as JSON. Existing handles remain valid. Returns how many attributes.
    #[instrument(level = "trace", skip(self))]
    pub fn drop_decoded(&mut self) -> usize {
        let mut dropped = 0;
        for (_, attr) in &mut self.attrs {
            if let Attr::Packed(packed) = attr {
                let json = packed.decoded.take().is_some();
                #[cfg(any(
                    feature = "allows",
                    feature = "cfg_hygiene",
                    feature = "cfg_scatter",
                    feature = "glob_imports",
                    feature = "inline_tests",
                    feature = "modularity",
                    feature = "pageantry",
                    feature = "derives",
                    feature = "visibility",
                    feature = "cli_layout"
                ))]
                let json = packed
                    .policy
                    .as_deref_mut()
                    .is_some_and(super::source_policy::Cache::clear)
                    || json;
                dropped += usize::from(json);
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
                let expanded = held.decoded.get().is_some();
                #[cfg(any(
                    feature = "allows",
                    feature = "cfg_hygiene",
                    feature = "cfg_scatter",
                    feature = "glob_imports",
                    feature = "inline_tests",
                    feature = "modularity",
                    feature = "pageantry",
                    feature = "derives",
                    feature = "visibility",
                    feature = "cli_layout"
                ))]
                let expanded = held
                    .policy
                    .as_deref()
                    .is_some_and(super::source_policy::Cache::is_decoded)
                    || expanded;
                decoded += usize::from(expanded);
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

    /// Whether a key is present, without decoding it.
    #[instrument(level = "trace", skip(self))]
    pub fn has_attr(&self, key: &str) -> bool {
        self.weight.has_attr(key)
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

    #[instrument(level = "trace", skip(self))]
    fn has_attr(&self, key: &str) -> bool {
        self.weight.has_attr(key)
    }
}
