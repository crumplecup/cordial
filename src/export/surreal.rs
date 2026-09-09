use serde_json::Value;

use crate::error::CordialResult;
use crate::ir::{CrateIr, CrateIrSnapshot, EdgeKind, NodeKind};

use tracing::instrument;
/// Agent-friendly graph export shaped for SurrealDB ingestion.
#[derive(Debug, Clone, derive_getters::Getters, derive_new::new, serde::Serialize)]
pub struct SurrealGraphExport {
    /// Cargo package name.
    crate_name: String,
    /// Graph nodes in this export.
    nodes: Vec<SurrealNode>,
    /// Directed edges in this graph or export.
    edges: Vec<SurrealEdge>,
}

/// One IR node in a SurrealDB-oriented export.
#[derive(Debug, Clone, derive_getters::Getters, derive_new::new, serde::Serialize)]
pub struct SurrealNode {
    /// Stable identifier.
    id: String,
    /// Node kind as a lowercase tag.
    kind: String,
    /// Optional item name.
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    /// JSON attributes attached to this node.
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    attrs: Value,
}

/// One IR edge in a SurrealDB-oriented export.
#[derive(Debug, Clone, derive_getters::Getters, derive_new::new, serde::Serialize)]
pub struct SurrealEdge {
    /// Source node id.
    from: String,
    /// Target node id.
    to: String,
    /// Edge kind as a lowercase tag.
    kind: String,
}

impl SurrealGraphExport {
    /// Rebuild from a serialized snapshot.
    #[instrument(level = "debug", skip(snapshot), ret)]
    pub fn from_snapshot(snapshot: &CrateIrSnapshot) -> Self {
        let nodes = snapshot
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                SurrealNode::new(
                    node_id(snapshot.crate_name.as_str(), index),
                    format_node_kind(&node.kind),
                    node.name.clone(),
                    attrs_to_json(&node.attrs),
                )
            })
            .collect();

        let edges = snapshot
            .edges
            .iter()
            .map(|(from, to, weight)| {
                SurrealEdge::new(
                    node_id(snapshot.crate_name.as_str(), *from as usize),
                    node_id(snapshot.crate_name.as_str(), *to as usize),
                    format_edge_kind(weight.kind()),
                )
            })
            .collect();

        Self::new(snapshot.crate_name.clone(), nodes, edges)
    }

    /// Build an export from a crate IR graph.
    #[instrument(level = "debug", skip(ir), err(level = "warn"))]
    pub fn from_crate_ir(ir: &CrateIr) -> CordialResult<Self> {
        Ok(Self::from_snapshot(&ir.snapshot()?))
    }

    /// Pretty-printed JSON for this export.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    pub fn to_json_pretty(&self) -> CordialResult<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

#[instrument(level = "debug")]
fn node_id(crate_name: &str, index: usize) -> String {
    format!("{crate_name}:node:{index}")
}

#[instrument(level = "debug", skip(kind))]
fn format_node_kind(kind: &NodeKind) -> String {
    match kind {
        NodeKind::Item(item) => format!("Item::{item:?}"),
        NodeKind::Plugin(name) => format!("Plugin({name})"),
        other => format!("{other:?}"),
    }
}

#[instrument(level = "debug", skip(kind))]
fn format_edge_kind(kind: EdgeKind) -> String {
    format!("{kind:?}")
}

#[instrument(level = "debug", skip(attrs))]
fn attrs_to_json(attrs: &[(String, Value)]) -> Value {
    if attrs.is_empty() {
        Value::Null
    } else {
        let mut map = serde_json::Map::new();
        for (key, value) in attrs {
            map.insert(key.clone(), value.clone());
        }
        Value::Object(map)
    }
}

/// Build SurrealDB-oriented CREATE statements for scripted import.
#[instrument(level = "debug", skip(export))]
pub fn surreal_statements(export: &SurrealGraphExport) -> Vec<String> {
    let mut statements = Vec::new();
    for node in export.nodes() {
        statements.push(format!(
            "CREATE {} SET kind = '{}', name = {}, attrs = {};",
            node.id(),
            escape_surreal(node.kind()),
            node.name()
                .as_deref()
                .map(|name| format!("'{name}'"))
                .unwrap_or_else(|| "NONE".to_string()),
            if node.attrs().is_null() {
                "NONE".to_string()
            } else {
                node.attrs().to_string()
            },
        ));
    }
    for edge in export.edges() {
        statements.push(format!(
            "RELATE {}->{}->{} SET kind = '{}';",
            edge.from(),
            edge.kind(),
            edge.to(),
            escape_surreal(edge.kind()),
        ));
    }
    statements
}

#[instrument(level = "debug")]
fn escape_surreal(value: &str) -> String {
    value.replace('\'', "\\'")
}
