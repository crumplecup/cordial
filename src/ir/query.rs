use crate::ir::{EdgeKind, NodeKind, NodeView};

use tracing::instrument;
/// Probe interest declaration compiled into graph traversals.
pub trait Query: Send + Sync {
    fn node_kinds(&self) -> &[NodeKind];
    fn edge_kinds(&self) -> &[EdgeKind];
    fn matches_node(&self, node: &dyn NodeView) -> bool;
}

/// Matches panic-site expression nodes materialized by [`crate::etiquettes::panics::PanicInventoryEnricher`].
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
    #[instrument(level = "debug", ret)]
    pub fn new() -> Self {
        Self::default()
    }

    #[instrument(level = "trace", skip(self, kinds))]
    pub fn node_kinds(mut self, kinds: impl IntoIterator<Item = NodeKind>) -> Self {
        self.node_kinds.extend(kinds);
        self
    }

    #[instrument(level = "trace", skip(self, kinds))]
    pub fn edge_kinds(mut self, kinds: impl IntoIterator<Item = EdgeKind>) -> Self {
        self.edge_kinds.extend(kinds);
        self
    }

    #[instrument(level = "trace", skip(self, key, value))]
    pub fn with_attr(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attr_key = Some(key.into());
        self.attr_value = Some(value.into());
        self
    }

    #[instrument(level = "trace", skip(self, key))]
    pub fn has_attr(mut self, key: impl Into<String>) -> Self {
        self.attr_key = Some(key.into());
        self.attr_value = None;
        self
    }

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
    pub node_kinds: Vec<NodeKind>,
    pub edge_kinds: Vec<EdgeKind>,
    pub attr_key: Option<String>,
    pub attr_value: Option<String>,
}

impl Query for BasicQuery {
    fn node_kinds(&self) -> &[NodeKind] {
        &self.node_kinds
    }

    fn edge_kinds(&self) -> &[EdgeKind] {
        &self.edge_kinds
    }

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
    #[instrument(level = "debug")]
    pub fn items() -> Self {
        QueryBuilder::new()
            .node_kinds([NodeKind::Item(crate::ir::ItemKind::Fn)])
            .build()
    }

    #[instrument(level = "debug")]
    pub fn all_nodes() -> Self {
        Self {
            node_kinds: Vec::new(),
            edge_kinds: Vec::new(),
            attr_key: None,
            attr_value: None,
        }
    }
}
