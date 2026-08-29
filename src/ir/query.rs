use crate::ir::{EdgeKind, NodeKind, NodeView};

use tracing::instrument;
/// Probe interest declaration compiled into graph traversals.
pub trait Query: Send + Sync {
    /// Node kinds.
    fn node_kinds(&self) -> &[NodeKind];
    /// Edge kinds.
    fn edge_kinds(&self) -> &[EdgeKind];
    /// Matches node.
    fn matches_node(&self, node: &dyn NodeView) -> bool;
}

/// Matches panic-site expression nodes (attr `panic_kind`) from the panics inventory.
#[derive(Debug, Default, Clone, Copy)]
pub struct PanicSitesQuery;

impl Query for PanicSitesQuery {
    fn node_kinds(&self) -> &[NodeKind] {
        &[NodeKind::Expr]
    }

    fn edge_kinds(&self) -> &[EdgeKind] {
        &[]
    }

    fn matches_node(&self, node: &dyn NodeView) -> bool {
        node.attr("panic_kind").is_some()
    }
}

/// Fluent builder for common probe queries.
#[derive(Debug, Default, Clone)]
pub struct QueryBuilder {
    node_kinds: Vec<NodeKind>,
    edge_kinds: Vec<EdgeKind>,
    attr_key: Option<String>,
    attr_value: Option<String>,
}

impl QueryBuilder {
    /// Construct a new value.
    #[instrument(level = "debug", ret)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Node kinds.
    #[instrument(level = "trace", skip(self, kinds))]
    pub fn node_kinds(mut self, kinds: impl IntoIterator<Item = NodeKind>) -> Self {
        self.node_kinds.extend(kinds);
        self
    }

    /// Edge kinds.
    #[instrument(level = "trace", skip(self, kinds))]
    pub fn edge_kinds(mut self, kinds: impl IntoIterator<Item = EdgeKind>) -> Self {
        self.edge_kinds.extend(kinds);
        self
    }

    /// Return a copy with `attr` set.
    #[instrument(level = "trace", skip(self, key, value))]
    pub fn with_attr(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attr_key = Some(key.into());
        self.attr_value = Some(value.into());
        self
    }

    /// Restrict matches to nodes that carry this attribute key (any value).
    #[instrument(level = "trace", skip(self, key))]
    pub fn has_attr(mut self, key: impl Into<String>) -> Self {
        self.attr_key = Some(key.into());
        self.attr_value = None;
        self
    }

    /// Finish the builder and return the value.
    #[instrument(level = "debug", skip(self))]
    pub fn build(self) -> BasicQuery {
        BasicQuery {
            node_kinds: self.node_kinds,
            edge_kinds: self.edge_kinds,
            attr_key: self.attr_key,
            attr_value: self.attr_value,
        }
    }
}

/// Concrete query used by built-in probes.
#[derive(Debug, Clone)]
pub struct BasicQuery {
    /// Node kinds this query matches.
    pub node_kinds: Vec<NodeKind>,
    /// Edge kinds this query traverses.
    pub edge_kinds: Vec<EdgeKind>,
    /// Optional attribute key this query filters on.
    pub attr_key: Option<String>,
    /// Optional attribute value this query filters on.
    pub attr_value: Option<String>,
}

impl Query for BasicQuery {
    #[instrument(level = "trace", skip(self))]
    fn node_kinds(&self) -> &[NodeKind] {
        &self.node_kinds
    }

    #[instrument(level = "trace", skip(self))]
    fn edge_kinds(&self) -> &[EdgeKind] {
        &self.edge_kinds
    }

    #[instrument(level = "trace", skip(self, node))]
    fn matches_node(&self, node: &dyn NodeView) -> bool {
        match (&self.attr_key, &self.attr_value) {
            (Some(key), Some(expected)) => node
                .attr(key)
                .and_then(|value| value.as_str())
                .is_some_and(|actual| actual == expected),
            (Some(key), None) => node.attr(key).is_some(),
            _ => true,
        }
    }
}

impl BasicQuery {
    /// Items.
    #[instrument(level = "debug")]
    pub fn items() -> Self {
        QueryBuilder::new()
            .node_kinds([NodeKind::Item(crate::ir::ItemKind::Fn)])
            .build()
    }

    /// Matches every node (no kind or attribute filter).
    pub const ALL_NODES: Self = Self {
        node_kinds: Vec::new(),
        edge_kinds: Vec::new(),
        attr_key: None,
        attr_value: None,
    };

    /// All nodes.
    #[instrument(level = "debug")]
    pub fn all_nodes() -> Self {
        Self::ALL_NODES
    }
}
