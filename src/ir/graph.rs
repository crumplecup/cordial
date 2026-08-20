use std::fs;
use std::path::{Path, PathBuf};

use petgraph::Direction;
use petgraph::stable_graph::StableDiGraph;
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::ir::{EdgeKind, EdgeWeight, IrIndexes, NodeId, NodeKind, NodeWeight};

/// Serializable snapshot of a crate graph for cache read/write.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateIrSnapshot {
    pub crate_name: String,
    pub root: NodeId,
    pub nodes: Vec<NodeWeight>,
    pub edges: Vec<(u32, u32, EdgeWeight)>,
    pub indexes: IrIndexes,
}

/// One crate's append-only IR graph.
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct CrateIr {
    #[getter(skip)]
    pub crate_name: String,
    #[getter(skip)]
    pub root: NodeId,
    graph: StableDiGraph<NodeWeight, EdgeWeight>,
    indexes: IrIndexes,
}

impl CrateIr {
    #[instrument(level = "debug", skip(crate_name), ret)]
    pub fn new(crate_name: impl Into<String>) -> Self {
        let crate_name = crate_name.into();
        let mut graph = StableDiGraph::new();
        let root_weight = NodeWeight::new(NodeKind::Crate).with_name(crate_name.clone());
        let root_index = graph.add_node(root_weight);
        let root = NodeId::from_index(root_index);
        Self {
            crate_name,
            root,
            graph,
            indexes: IrIndexes::default(),
        }
    }

    #[instrument(level = "trace", skip(self, id))]
    pub fn node_weight(&self, id: NodeId) -> Option<&NodeWeight> {
        self.graph.node_weight(id.to_index())
    }

    #[instrument(level = "debug", skip(self, weight))]
    pub fn insert_node(&mut self, weight: NodeWeight) -> NodeId {
        let index = self.graph.add_node(weight);
        let id = NodeId::from_index(index);
        if let Some(node_weight) = self.graph.node_weight(index) {
            self.indexes.index_node(id, node_weight);
        }
        id
    }

    #[instrument(level = "debug", skip(self, from, to, kind), err(level = "warn"))]
    pub fn insert_edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) -> CordialResult<()> {
        if self.graph.node_weight(from.to_index()).is_none()
            || self.graph.node_weight(to.to_index()).is_none()
        {
            return Err(CordialError::invariant("edge references missing node"));
        }
        self.graph
            .add_edge(from.to_index(), to.to_index(), EdgeWeight::new(kind));
        Ok(())
    }

    #[instrument(level = "trace", skip(self, node, value), err(level = "warn"))]
    pub fn set_attr(
        &mut self,
        node: NodeId,
        key: &str,
        value: serde_json::Value,
    ) -> CordialResult<()> {
        let weight = self
            .graph
            .node_weight_mut(node.to_index())
            .ok_or_else(|| CordialError::invariant("set_attr on missing node"))?;
        weight.set_attr(key, value);
        if let Some(node_weight) = self.graph.node_weight(node.to_index()) {
            self.indexes.index_node(node, node_weight);
        }
        Ok(())
    }

    #[instrument(level = "debug", skip(self))]
    pub fn rebuild_path_index(&mut self) {
        self.indexes.rebuild_by_path(&self.graph);
    }

    #[instrument(level = "debug", skip(self, node, kind, direction))]
    pub fn neighbors(
        &self,
        node: NodeId,
        kind: EdgeKind,
        direction: Direction,
    ) -> Vec<(EdgeWeight, NodeId)> {
        let index = node.to_index();
        self.graph
            .edges_directed(index, direction)
            .filter(|edge| edge.weight().kind == kind)
            .map(|edge| {
                let target = match direction {
                    Direction::Outgoing => edge.target(),
                    Direction::Incoming => edge.source(),
                };
                (edge.weight().clone(), NodeId::from_index(target))
            })
            .collect()
    }

    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    pub fn snapshot(&self) -> CordialResult<CrateIrSnapshot> {
        Ok(CrateIrSnapshot {
            crate_name: self.crate_name.clone(),
            root: self.root,
            nodes: self.graph.node_weights().cloned().collect(),
            edges: self
                .graph
                .edge_indices()
                .map(|edge| {
                    let (from, to) = self.graph.edge_endpoints(edge).ok_or_else(|| {
                        CordialError::invariant("snapshot edge missing endpoints")
                    })?;
                    Ok((
                        from.index() as u32,
                        to.index() as u32,
                        self.graph[edge].clone(),
                    ))
                })
                .collect::<CordialResult<_>>()?,
            indexes: self.indexes.clone(),
        })
    }

    #[instrument(level = "debug", skip(snapshot), err(level = "warn"))]
    pub fn from_snapshot(snapshot: CrateIrSnapshot) -> CordialResult<Self> {
        let mut graph = StableDiGraph::new();
        let mut index_map = Vec::with_capacity(snapshot.nodes.len());

        for node in snapshot.nodes {
            index_map.push(graph.add_node(node));
        }

        for (from, to, weight) in snapshot.edges {
            let from_index = index_map
                .get(from as usize)
                .copied()
                .ok_or_else(|| CordialError::invariant("snapshot edge from invalid"))?;
            let to_index = index_map
                .get(to as usize)
                .copied()
                .ok_or_else(|| CordialError::invariant("snapshot edge to invalid"))?;
            graph.add_edge(from_index, to_index, weight);
        }

        let root = snapshot.root;
        Ok(Self {
            crate_name: snapshot.crate_name,
            root,
            graph,
            indexes: snapshot.indexes,
        })
    }

    #[instrument(level = "info", skip(self, path), err(level = "warn"))]
    pub fn write_cache(&self, path: &Path) -> CordialResult<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let snapshot = self.snapshot()?;
        let json = serde_json::to_string_pretty(&snapshot)?;
        fs::write(path, json)?;
        Ok(())
    }

    #[instrument(level = "info", skip(path), err(level = "warn"))]
    pub fn read_cache(path: &Path) -> CordialResult<Self> {
        let json = fs::read_to_string(path)?;
        let snapshot: CrateIrSnapshot = serde_json::from_str(&json)?;
        Self::from_snapshot(snapshot)
    }

    #[instrument(level = "debug")]
    pub fn cache_path(cache_dir: &Path, crate_name: &str) -> PathBuf {
        cache_dir.join(format!("{crate_name}.ir.json"))
    }
}
