use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::EdgeWeight;
use super::node::{NodeId, NodeWeight};

use tracing::instrument;
/// String key for an IR node attribute.
pub type AttrKey = String;

/// JSON value stored on an IR node attribute.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AttrValue(pub serde_json::Value);

/// Fully-qualified Rust path used for indexing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QualifiedPath(pub Vec<String>);

impl QualifiedPath {
    /// Build a path from path segments.
    #[instrument(level = "debug", skip(segments), ret)]
    pub fn from_segments(segments: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self(segments.into_iter().map(Into::into).collect())
    }

    /// Stable string form of this value.
    #[instrument(level = "trace", skip(self))]
    pub fn as_str(&self) -> String {
        self.0.join("::")
    }
}

impl From<&str> for QualifiedPath {
    #[instrument(level = "debug", ret)]
    fn from(value: &str) -> Self {
        Self::from_segments(value.split("::"))
    }
}

/// Secondary indexes over a crate graph.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct IrIndexes {
    /// Node ids keyed by `foo::bar` path.
    pub by_path: HashMap<String, NodeId>,
    /// Node ids grouped by kind tag.
    pub by_kind: HashMap<String, Vec<NodeId>>,
}

impl IrIndexes {
    /// Index node.
    #[instrument(level = "debug", skip(self, node, weight))]
    pub fn index_node(&mut self, node: NodeId, weight: &NodeWeight) {
        let kind_key = format!("{:?}", weight.kind);
        self.by_kind.entry(kind_key).or_default().push(node);

        if let Some(path) = weight.attr("qualified_path").and_then(|v| v.as_str()) {
            self.by_path.insert(path.to_string(), node);
        }
    }

    /// Rebuild the qualified-path index from all nodes in the graph.
    ///
    /// When multiple nodes share a path, prefer rustdoc inventory nodes over source nodes.
    #[instrument(level = "debug", skip(self, graph))]
    pub fn rebuild_by_path(
        &mut self,
        graph: &petgraph::stable_graph::StableDiGraph<NodeWeight, EdgeWeight>,
    ) {
        self.by_path.clear();
        let mut preferred: std::collections::BTreeMap<String, (NodeId, u8)> =
            std::collections::BTreeMap::new();
        for index in graph.node_indices() {
            let node = NodeId::from_index(index);
            let Some(weight) = graph.node_weight(index) else {
                continue;
            };
            let Some(path) = weight.attr("qualified_path").and_then(|v| v.as_str()) else {
                continue;
            };
            let priority = match weight.attr("ir_origin").and_then(|v| v.as_str()) {
                Some("rustdoc") => 2,
                Some("source") => 1,
                _ => 0,
            };
            if preferred
                .get(path)
                .is_none_or(|(_, existing)| priority >= *existing)
            {
                preferred.insert(path.to_string(), (node, priority));
            }
        }
        self.by_path = preferred
            .into_iter()
            .map(|(path, (node, _))| (path, node))
            .collect();
    }
}
