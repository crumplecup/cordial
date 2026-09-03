use serde_json::Value;

use crate::error::CordialResult;
use crate::ir::{CrateIr, CrateIrSnapshot, EdgeKind, NodeKind};

use tracing::instrument;
/// Agent-friendly graph export shaped for SurrealDB ingestion.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SurrealGraphExport {
    /// Cargo package name.
    pub crate_name: String,
    /// Graph nodes in this export.
    pub nodes: Vec<SurrealNode>,
    /// Directed edges in this graph or export.
    pub edges: Vec<SurrealEdge>,
}

/// One IR node in a SurrealDB-oriented export.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SurrealNode {
    /// Stable identifier.
    pub id: String,
    /// Node kind as a lowercase tag.
    pub kind: String,
    /// Optional item name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// JSON attributes attached to this node.
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub attrs: Value,
}

/// One IR edge in a SurrealDB-oriented export.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SurrealEdge {
    /// Source node id.
    pub from: String,
    /// Target node id.
    pub to: String,
    /// Edge kind as a lowercase tag.
    pub kind: String,
}

impl SurrealGraphExport {
    /// Rebuild from a serialized snapshot.
    #[instrument(level = "debug", skip(snapshot), ret)]
    pub fn from_snapshot(snapshot: &CrateIrSnapshot) -> Self {
        let nodes = snapshot
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| SurrealNode {
                id: node_id(snapshot.crate_name.as_str(), index),
                kind: format_node_kind(&node.kind),
                name: node.name.clone(),
                attrs: attrs_to_json(&node.attrs),
            })
            .collect();

        let edges = snapshot
            .edges
            .iter()
            .map(|(from, to, weight)| SurrealEdge {
                from: node_id(snapshot.crate_name.as_str(), *from as usize),
                to: node_id(snapshot.crate_name.as_str(), *to as usize),
                kind: format_edge_kind(weight.kind()),
            })
            .collect();

        Self {
            crate_name: snapshot.crate_name.clone(),
            nodes,
            edges,
        }
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
    for node in &export.nodes {
        statements.push(format!(
            "CREATE {} SET kind = '{}', name = {}, attrs = {};",
            node.id,
            escape_surreal(&node.kind),
            node.name
                .as_deref()
                .map(|name| format!("'{name}'"))
                .unwrap_or_else(|| "NONE".to_string()),
            if node.attrs.is_null() {
                "NONE".to_string()
            } else {
                node.attrs.to_string()
            },
        ));
    }
    for edge in &export.edges {
        statements.push(format!(
            "RELATE {}->{}->{} SET kind = '{}';",
            edge.from,
            edge.kind,
            edge.to,
            escape_surreal(&edge.kind),
        ));
    }
    statements
}

#[instrument(level = "debug")]
fn escape_surreal(value: &str) -> String {
    value.replace('\'', "\\'")
}
