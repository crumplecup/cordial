use crate::error::CordialResult;
use crate::ir::{CrateIr, EdgeKind, NodeId, NodeKind, NodeWeight};
#[cfg(feature = "impl_coverage")]
use crate::rustdoc::WrapperCoverageMap;
use tracing::instrument;

use super::query::Query;

/// Read-only view over a crate IR graph.
pub trait IrView {
    fn crate_name(&self) -> &str;
    fn root(&self) -> CordialResult<NodeId>;
    fn node(&self, id: NodeId) -> Option<NodeRef<'_>>;
    fn nodes_matching(&self, query: &dyn Query) -> Vec<NodeRef<'_>>;
    fn parents(&self, id: NodeId, kind: EdgeKind) -> Vec<NodeId>;
    fn children(&self, id: NodeId, kind: EdgeKind) -> Vec<NodeId>;
    fn node_by_path(&self, path: &str) -> Option<NodeId>;
}

/// Mutable view for enrichers.
pub trait IrMut: IrView {
    fn insert_node(&mut self, weight: NodeWeight) -> CordialResult<NodeId>;
    fn insert_edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) -> CordialResult<()>;
    fn set_attr(&mut self, node: NodeId, key: &str, value: serde_json::Value) -> CordialResult<()>;
    fn rebuild_path_index(&mut self) -> CordialResult<()>;

    /// Workspace-level wrapper coverage from the elicitation hub IR.
    #[cfg(feature = "impl_coverage")]
    fn workspace_wrapper_coverage(&self) -> Option<&WrapperCoverageMap> {
        None
    }
}

/// Borrowed node handle for probes.
pub struct NodeRef<'a> {
    pub id: NodeId,
    pub weight: &'a NodeWeight,
}

impl<'a> NodeRef<'a> {
    #[instrument(level = "trace", skip(self))]
    pub fn kind(&self) -> &NodeKind {
        &self.weight.kind
    }

    #[instrument(level = "trace", skip(self))]
    pub fn attr(&self, key: &str) -> Option<&serde_json::Value> {
        self.weight.attr(key)
    }
}

/// Trait alias for node-level read API used by probes.
pub trait NodeView {
    fn id(&self) -> NodeId;
    fn kind(&self) -> &NodeKind;
    fn attr(&self, key: &str) -> Option<&serde_json::Value>;
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
