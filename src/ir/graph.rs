use std::fs;
use std::path::{Path, PathBuf};

use petgraph::Direction;
use petgraph::stable_graph::StableDiGraph;
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::ir::{EdgeKind, EdgeWeight, IrIndexes, NodeId, NodeKind, NodeWeight};

/// zstd level for IR files: fast to write, still an order of magnitude smaller than JSON.
const ZSTD_LEVEL: i32 = 3;

/// Serializable snapshot of a crate graph for cache read/write.
#[derive(Debug, Clone, Serialize, Deserialize, derive_getters::Getters)]
pub struct CrateIrSnapshot {
    /// Cargo package name.
    crate_name: String,
    /// Filesystem or graph root.
    #[getter(copy)]
    root: NodeId,
    /// Graph nodes in this export.
    nodes: Vec<NodeWeight>,
    /// Directed edges in this graph or export.
    edges: Vec<(u32, u32, EdgeWeight)>,
    /// Path and kind indexes for this snapshot.
    indexes: IrIndexes,
}

/// One crate's append-oriented IR graph.
///
/// Loaders create the initial graph, enrichers add nodes/edges/attributes, and
/// probes read it through [`IrView`](crate::IrView). Node ids are stable within
/// one graph snapshot but are not a persistent cross-run identifier.
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct CrateIr {
    /// Cargo package name.
    crate_name: String,
    /// Filesystem or graph root.
    #[getter(copy)]
    root: NodeId,
    graph: StableDiGraph<NodeWeight, EdgeWeight>,
    indexes: IrIndexes,
}

impl CrateIr {
    /// Construct an empty crate graph with a crate root node.
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

    /// Borrow the stored payload for a node id.
    #[instrument(level = "trace", skip(self, id))]
    pub fn node_weight(&self, id: NodeId) -> Option<&NodeWeight> {
        self.graph.node_weight(id.to_index())
    }

    /// Insert a node and return its id.
    #[instrument(level = "debug", skip(self, weight))]
    pub fn insert_node(&mut self, weight: NodeWeight) -> NodeId {
        let index = self.graph.add_node(weight);
        let id = NodeId::from_index(index);
        if let Some(node_weight) = self.graph.node_weight(index) {
            self.indexes.index_node(id, node_weight);
        }
        id
    }

    /// Insert a directed edge of `kind`.
    #[instrument(level = "debug", skip(self, from, to, kind), err(level = "warn"))]
    pub fn insert_edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) -> CordialResult<()> {
        if self.graph.node_weight(from.to_index()).is_none()
            || self.graph.node_weight(to.to_index()).is_none()
        {
            return Err(CordialError::edge_references_missing_node());
        }
        self.graph
            .add_edge(from.to_index(), to.to_index(), EdgeWeight::new(kind));
        Ok(())
    }

    /// Insert a `ContextCalls` edge resolved in the non-ordinary `context`.
    #[instrument(level = "debug", skip(self, from, to, context), err(level = "warn"))]
    pub fn insert_context_call(
        &mut self,
        from: NodeId,
        to: NodeId,
        context: crate::ir::SemanticContextKey,
    ) -> CordialResult<()> {
        if self.graph.node_weight(from.to_index()).is_none()
            || self.graph.node_weight(to.to_index()).is_none()
        {
            return Err(CordialError::edge_references_missing_node());
        }
        self.graph.add_edge(
            from.to_index(),
            to.to_index(),
            EdgeWeight::new(EdgeKind::ContextCalls).with_context(context),
        );
        Ok(())
    }

    /// Set a JSON attribute on a node.
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
            .ok_or_else(|| CordialError::set_attr_on_missing_node())?;
        weight.set_attr(key, value);
        if let Some(node_weight) = self.graph.node_weight(node.to_index()) {
            self.indexes.index_node(node, node_weight);
        }
        Ok(())
    }

    /// Rebuild the path → node index after structural edits.
    #[instrument(level = "debug", skip(self))]
    pub fn rebuild_path_index(&mut self) {
        self.indexes.rebuild_by_path(&self.graph);
    }

    /// Return neighboring nodes connected by `kind` in `direction`.
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
            .filter(|edge| edge.weight().kind() == kind)
            .map(|edge| {
                let target = match direction {
                    Direction::Outgoing => edge.target(),
                    Direction::Incoming => edge.source(),
                };
                (edge.weight().clone(), NodeId::from_index(target))
            })
            .collect()
    }

    /// Serialize the graph and indexes for cache storage.
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
                    let (from, to) = self
                        .graph
                        .edge_endpoints(edge)
                        .ok_or_else(|| CordialError::snapshot_edge_missing_endpoints())?;
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

    /// Rebuild from a serialized snapshot.
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
                .ok_or_else(|| CordialError::snapshot_edge_from_invalid())?;
            let to_index = index_map
                .get(to as usize)
                .copied()
                .ok_or_else(|| CordialError::snapshot_edge_to_invalid())?;
            graph.add_edge(from_index, to_index, weight);
        }

        let root = snapshot.root;
        let mut indexes = snapshot.indexes;
        indexes.rebuild_by_path(&graph);
        Ok(Self {
            crate_name: snapshot.crate_name,
            root,
            graph,
            indexes,
        })
    }

    /// Write the IR as compact JSON compressed with zstd.
    ///
    /// Compact and compressed because the pretty-printed form of a real crate
    /// runs to hundreds of megabytes; this stays a small fraction of that.
    #[instrument(level = "info", skip(self, path), err(level = "warn"))]
    #[hotpath::measure(impl_type = "CrateIr")]
    pub fn write_cache(&self, path: &Path) -> CordialResult<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let started = std::time::Instant::now();
        let snapshot = self.snapshot()?;
        let mut encoder = zstd::stream::write::Encoder::new(fs::File::create(path)?, ZSTD_LEVEL)?;
        serde_json::to_writer(&mut encoder, &snapshot)?;
        encoder.finish()?;
        tracing::debug!(
            target: "cordial::metrics",
            path = %path.display(),
            bytes = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0),
            nodes = snapshot.nodes().len(),
            edges = snapshot.edges().len(),
            millis = started.elapsed().as_millis() as u64,
            "ir file written"
        );
        Ok(())
    }

    /// Read an IR file written by [`Self::write_cache`].
    #[instrument(level = "info", skip(path), err(level = "warn"))]
    #[hotpath::measure(impl_type = "CrateIr")]
    pub fn read_cache(path: &Path) -> CordialResult<Self> {
        let started = std::time::Instant::now();
        let decoder = zstd::stream::read::Decoder::new(fs::File::open(path)?)?;
        let snapshot: CrateIrSnapshot = serde_json::from_reader(std::io::BufReader::new(decoder))?;
        let decoded = started.elapsed();
        let (nodes, edges) = (snapshot.nodes().len(), snapshot.edges().len());
        let ir = Self::from_snapshot(snapshot)?;
        tracing::debug!(
            target: "cordial::metrics",
            path = %path.display(),
            bytes = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0),
            nodes,
            edges,
            decode_millis = decoded.as_millis() as u64,
            total_millis = started.elapsed().as_millis() as u64,
            "ir file read"
        );
        Ok(ir)
    }

    /// Store path for this crate's IR cache file.
    #[instrument(level = "debug")]
    pub fn cache_path(cache_dir: &Path, crate_name: &str) -> PathBuf {
        cache_dir.join(format!("{crate_name}.ir.zst"))
    }
}
