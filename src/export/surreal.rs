use serde_json::Value;

use crate::error::CordialResult;
use crate::ir::{CrateIr, CrateIrSnapshot, EdgeKind, NodeKind};

/// Agent-friendly graph export shaped for SurrealDB ingestion.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SurrealGraphExport {
    pub crate_name: String,
    pub nodes: Vec<SurrealNode>,
    pub edges: Vec<SurrealEdge>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SurrealNode {
    pub id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub attrs: Value,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SurrealEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
}

impl SurrealGraphExport {
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
                kind: format_edge_kind(weight.kind),
            })
            .collect();

        Self {
            crate_name: snapshot.crate_name.clone(),
            nodes,
            edges,
        }
    }

    pub fn from_crate_ir(ir: &CrateIr) -> CordialResult<Self> {
        Ok(Self::from_snapshot(&ir.snapshot()?))
    }

    pub fn to_json_pretty(&self) -> CordialResult<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

fn node_id(crate_name: &str, index: usize) -> String {
    format!("{crate_name}:node:{index}")
}

fn format_node_kind(kind: &NodeKind) -> String {
    match kind {
        NodeKind::Item(item) => format!("Item::{item:?}"),
        NodeKind::Plugin(name) => format!("Plugin({name})"),
        other => format!("{other:?}"),
    }
}

fn format_edge_kind(kind: EdgeKind) -> String {
    format!("{kind:?}")
}

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

fn escape_surreal(value: &str) -> String {
    value.replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    use miette::{IntoDiagnostic, WrapErr};

    use super::*;
    use crate::ir::CrateIr;

    #[test]
    fn export_includes_root_node() -> miette::Result<()> {
        let ir = CrateIr::new("demo");
        let export = SurrealGraphExport::from_crate_ir(&ir)
            .into_diagnostic()
            .wrap_err("snapshot")?;
        assert_eq!(export.crate_name, "demo");
        assert!(!export.nodes.is_empty());
        assert!(export.nodes[0].id.starts_with("demo:node:"));
        Ok(())
    }

    #[test]
    fn surreal_statements_non_empty() -> miette::Result<()> {
        let ir = CrateIr::new("demo");
        let export = SurrealGraphExport::from_crate_ir(&ir)
            .into_diagnostic()
            .wrap_err("snapshot")?;
        let statements = surreal_statements(&export);
        assert!(!statements.is_empty());
        assert!(statements[0].starts_with("CREATE demo:node:"));
        Ok(())
    }
}
