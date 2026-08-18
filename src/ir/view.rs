use crate::error::CordialResult;
use crate::ir::{CrateIr, EdgeKind, NodeId, NodeKind, NodeWeight};
use tracing::instrument;
#[cfg(feature = "impl_coverage")]
use crate::rustdoc::WrapperCoverageMap;

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

    #[instrument(level = "trace", skip(self, key))]
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
    fn id(&self) -> NodeId {
        self.id
    }

    fn kind(&self) -> &NodeKind {
        &self.weight.kind
    }

    fn attr(&self, key: &str) -> Option<&serde_json::Value> {
        self.weight.attr(key)
    }
}

impl IrView for CrateIr {
    fn crate_name(&self) -> &str {
        &self.crate_name
    }

    fn root(&self) -> CordialResult<NodeId> {
        Ok(self.root)
    }

    fn node(&self, id: NodeId) -> Option<NodeRef<'_>> {
        self.node_weight(id).map(|weight| NodeRef { id, weight })
    }

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

    fn parents(&self, id: NodeId, kind: EdgeKind) -> Vec<NodeId> {
        self.neighbors(id, kind, petgraph::Direction::Incoming)
            .into_iter()
            .map(|(_, parent)| parent)
            .collect()
    }

    fn children(&self, id: NodeId, kind: EdgeKind) -> Vec<NodeId> {
        self.neighbors(id, kind, petgraph::Direction::Outgoing)
            .into_iter()
            .map(|(_, child)| child)
            .collect()
    }

    fn node_by_path(&self, path: &str) -> Option<NodeId> {
        self.indexes().by_path.get(path).copied()
    }
}

impl IrMut for CrateIr {
    fn insert_node(&mut self, weight: NodeWeight) -> CordialResult<NodeId> {
        Ok(CrateIr::insert_node(self, weight))
    }

    fn insert_edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) -> CordialResult<()> {
        CrateIr::insert_edge(self, from, to, kind)
    }

    fn set_attr(&mut self, node: NodeId, key: &str, value: serde_json::Value) -> CordialResult<()> {
        CrateIr::set_attr(self, node, key, value)
    }

    fn rebuild_path_index(&mut self) -> CordialResult<()> {
        CrateIr::rebuild_path_index(self);
        Ok(())
    }
}
