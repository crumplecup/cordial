use crate::error::CordialResult;
use crate::ir::{CrateIr, EdgeKind, NodeId, NodeKind, NodeWeight};
#[cfg(feature = "impl_coverage")]
use crate::rustdoc::WrapperCoverageMap;
use tracing::instrument;

use super::query::Query;

/// Read-only view over one crate's IR graph.
///
/// Probes, assessors, and reporters use this trait instead of depending on the
/// concrete petgraph storage. All node ids are local to this crate graph and
/// remain valid only for the IR snapshot passed to the hook.
pub trait IrView {
    /// Cargo package name this crate IR belongs to.
    fn crate_name(&self) -> &str;
    /// Root node of this crate graph.
    fn root(&self) -> CordialResult<NodeId>;
    /// Borrow the node with this id, if it exists in this crate graph.
    fn node(&self, id: NodeId) -> Option<NodeRef<'_>>;
    /// Return nodes whose kinds and attributes match `query`.
    ///
    /// The default concrete implementation uses [`Query::node_kinds`] as a
    /// cheap prefilter, then calls [`Query::matches_node`] for the final
    /// predicate.
    fn nodes_matching(&self, query: &dyn Query) -> Vec<NodeRef<'_>>;
    /// Parent node ids along incoming edges of `kind`.
    fn parents(&self, id: NodeId, kind: EdgeKind) -> Vec<NodeId>;
    /// Child node ids along outgoing edges of `kind`.
    fn children(&self, id: NodeId, kind: EdgeKind) -> Vec<NodeId>;
    /// Node id for a `foo::bar` path, if the path index knows it.
    fn node_by_path(&self, path: &str) -> Option<NodeId>;
}

/// Mutable crate IR view for enrichers.
///
/// Enrichers should add facts and then rebuild affected indexes before later
/// probes rely on lookup by qualified path.
pub trait IrMut: IrView {
    /// Insert a node and return its crate-local id.
    fn insert_node(&mut self, weight: NodeWeight) -> CordialResult<NodeId>;
    /// Insert a directed edge of `kind` between existing nodes.
    fn insert_edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) -> CordialResult<()>;
    /// Append a JSON attribute value to a node.
    ///
    /// Attribute lookup returns the latest value for a key, so enrichers may
    /// refine loader facts without mutating historical entries in place.
    fn set_attr(&mut self, node: NodeId, key: &str, value: serde_json::Value) -> CordialResult<()>;
    /// Rebuild the path → node index after structural edits.
    fn rebuild_path_index(&mut self) -> CordialResult<()>;

    /// Workspace-level wrapper coverage from the elicitation hub IR.
    #[cfg(feature = "impl_coverage")]
    fn workspace_wrapper_coverage(&self) -> Option<&WrapperCoverageMap> {
        None
    }
}

/// Node-level read API used by queries and probes.
pub trait NodeView {
    /// Crate-local node id.
    fn id(&self) -> NodeId;
    /// Node kind.
    fn kind(&self) -> &NodeKind;
    /// Latest attribute value stored under `key`, if present.
    fn attr(&self, key: &str) -> Option<&serde_json::Value>;
}

/// Borrowed node handle returned by [`IrView`] queries.
pub struct NodeRef<'a> {
    /// Crate-local node id.
    pub id: NodeId,
    /// Borrowed node payload.
    pub weight: &'a NodeWeight,
}

impl<'a> NodeRef<'a> {
    /// Node kind.
    #[instrument(level = "trace", skip(self))]
    pub fn kind(&self) -> &NodeKind {
        &self.weight.kind
    }

    /// Latest attribute value stored under `key`, if present.
    #[instrument(level = "trace", skip(self))]
    pub fn attr(&self, key: &str) -> Option<&serde_json::Value> {
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
        &self.weight.kind
    }

    #[instrument(level = "trace", skip(self))]
    fn attr(&self, key: &str) -> Option<&serde_json::Value> {
        self.weight.attr(key)
    }
}

impl IrView for CrateIr {
    #[instrument(level = "trace", skip(self))]
    fn crate_name(&self) -> &str {
        &self.crate_name
    }

    #[instrument(level = "trace", skip(self))]
    fn root(&self) -> CordialResult<NodeId> {
        Ok(self.root)
    }

    #[instrument(level = "trace", skip(self, id))]
    fn node(&self, id: NodeId) -> Option<NodeRef<'_>> {
        self.node_weight(id).map(|weight| NodeRef { id, weight })
    }

    #[instrument(level = "trace", skip(self, query))]
    fn nodes_matching(&self, query: &dyn Query) -> Vec<NodeRef<'_>> {
        let kinds = query.node_kinds();
        self.graph()
            .node_indices()
            .filter_map(|index| {
                let id = NodeId::from_index(index);
                let weight = self.graph().node_weight(index)?;
                if kinds.is_empty() || kinds.iter().any(|kind| kind == &weight.kind) {
                    Some(NodeRef { id, weight })
                } else {
                    None
                }
            })
            .filter(|node| query.matches_node(node))
            .collect()
    }

    #[instrument(level = "trace", skip(self, id, kind))]
    fn parents(&self, id: NodeId, kind: EdgeKind) -> Vec<NodeId> {
        self.neighbors(id, kind, petgraph::Direction::Incoming)
            .into_iter()
            .map(|(_, parent)| parent)
            .collect()
    }

    #[instrument(level = "trace", skip(self, id, kind))]
    fn children(&self, id: NodeId, kind: EdgeKind) -> Vec<NodeId> {
        self.neighbors(id, kind, petgraph::Direction::Outgoing)
            .into_iter()
            .map(|(_, child)| child)
            .collect()
    }

    #[instrument(level = "trace", skip(self, path))]
    fn node_by_path(&self, path: &str) -> Option<NodeId> {
        self.indexes().by_path.get(path).copied()
    }
}

impl IrMut for CrateIr {
    #[instrument(level = "trace", skip(self, weight))]
    fn insert_node(&mut self, weight: NodeWeight) -> CordialResult<NodeId> {
        Ok(CrateIr::insert_node(self, weight))
    }

    #[instrument(level = "trace", skip(self, from, to, kind))]
    fn insert_edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) -> CordialResult<()> {
        CrateIr::insert_edge(self, from, to, kind)
    }

    #[instrument(level = "trace", skip(self, node, value), err(level = "warn"))]
    fn set_attr(&mut self, node: NodeId, key: &str, value: serde_json::Value) -> CordialResult<()> {
        CrateIr::set_attr(self, node, key, value)
    }

    #[instrument(level = "trace", skip(self))]
    fn rebuild_path_index(&mut self) -> CordialResult<()> {
        CrateIr::rebuild_path_index(self);
        Ok(())
    }
}
