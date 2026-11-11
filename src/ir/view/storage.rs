//! Concrete graph storage implementations of the public IR views.

use super::{IrMut, IrView};
use crate::CordialResult;
use crate::ir::{CrateIr, EdgeKind, NodeId, NodeRef, NodeWeight, Query};
use tracing::instrument;

impl IrView for CrateIr {
    #[instrument(level = "trace", skip(self))]
    fn crate_name(&self) -> &str {
        self.crate_name()
    }

    #[instrument(level = "trace", skip(self))]
    fn root(&self) -> CordialResult<NodeId> {
        Ok(self.root())
    }

    #[instrument(level = "trace", skip(self, id))]
    fn node(&self, id: NodeId) -> Option<NodeRef<'_>> {
        self.node_weight(id).map(|weight| NodeRef::new(id, weight))
    }

    #[instrument(level = "trace", skip(self, query))]
    fn nodes_matching(&self, query: &dyn Query) -> Vec<NodeRef<'_>> {
        let kinds = query.node_kinds();
        self.graph()
            .node_indices()
            .filter_map(|index| {
                let id = NodeId::from_index(index);
                let weight = self.graph().node_weight(index)?;
                if kinds.is_empty() || kinds.iter().any(|kind| kind == weight.kind()) {
                    Some(NodeRef::new(id, weight))
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

    #[instrument(level = "trace", skip(self, id, context))]
    fn context_parents(&self, id: NodeId, context: &crate::ir::SemanticContextKey) -> Vec<NodeId> {
        self.neighbors(id, EdgeKind::ContextCalls, petgraph::Direction::Incoming)
            .into_iter()
            .filter(|(weight, _)| weight.context().as_ref() == Some(context))
            .map(|(_, parent)| parent)
            .collect()
    }

    #[instrument(level = "trace", skip(self, id, context))]
    fn context_children(&self, id: NodeId, context: &crate::ir::SemanticContextKey) -> Vec<NodeId> {
        self.neighbors(id, EdgeKind::ContextCalls, petgraph::Direction::Outgoing)
            .into_iter()
            .filter(|(weight, _)| weight.context().as_ref() == Some(context))
            .map(|(_, child)| child)
            .collect()
    }

    #[instrument(level = "trace", skip(self, path))]
    fn node_by_path(&self, path: &str) -> Option<NodeId> {
        self.indexes().by_path().get(path).copied()
    }

    #[instrument(level = "trace", skip(self, path))]
    fn nodes_by_path(&self, path: &str) -> Vec<NodeId> {
        self.indexes()
            .declarations_by_path()
            .get(path)
            .cloned()
            .unwrap_or_default()
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
