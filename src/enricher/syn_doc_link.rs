//! Links syn source inventory nodes to rustdoc item nodes by qualified path.

use std::collections::BTreeMap;

use crate::error::CordialResult;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{
    ATTR_IR_ORIGIN, ATTR_SYN_DOC_PEER, BasicQuery, EdgeKind, IrMut, NodeKind, NodeView,
    ORIGIN_RUSTDOC, ORIGIN_SOURCE,
};

use tracing::instrument;
/// Links syn and rustdoc item nodes that share a `qualified_path`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SynDocLinkEnricher;

impl SynDocLinkEnricher {
    pub const ID: &'static str = "syn-doc-link";
}

impl IrEnricher for SynDocLinkEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        2
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let ir = view.ir;

        let crate_name = ir.crate_name();
        let mut by_path: BTreeMap<String, (Option<crate::ir::NodeId>, Option<crate::ir::NodeId>)> =
            BTreeMap::new();

        for node in ir
            .nodes_matching(&BasicQuery::all_nodes())
            .into_iter()
            .filter(|node| matches!(node.kind(), NodeKind::Item(_)))
        {
            let Some(path) = node.attr("qualified_path").and_then(|value| value.as_str()) else {
                continue;
            };
            let key = inventory_link_key(path, crate_name);
            let entry = by_path.entry(key).or_default();
            match node.attr(ATTR_IR_ORIGIN).and_then(|value| value.as_str()) {
                Some(ORIGIN_SOURCE) => entry.0 = Some(node.id),
                Some(ORIGIN_RUSTDOC) => entry.1 = Some(node.id),
                _ => {}
            }
        }

        for (source, rustdoc) in by_path.into_values() {
            let (Some(source), Some(rustdoc)) = (source, rustdoc) else {
                continue;
            };
            link_peers(ir, source, rustdoc)?;
        }

        ir.rebuild_path_index()?;
        Ok(())
    }
}

#[instrument(level = "debug", skip(ir, source, rustdoc), err(level = "warn"))]
fn link_peers(
    ir: &mut dyn IrMut,
    source: crate::ir::NodeId,
    rustdoc: crate::ir::NodeId,
) -> CordialResult<()> {
    ir.set_attr(
        source,
        ATTR_SYN_DOC_PEER,
        serde_json::Value::Number(rustdoc.0.into()),
    )?;
    ir.set_attr(
        rustdoc,
        ATTR_SYN_DOC_PEER,
        serde_json::Value::Number(source.0.into()),
    )?;
    ir.insert_edge(source, rustdoc, EdgeKind::Plugin)?;
    ir.insert_edge(rustdoc, source, EdgeKind::Plugin)?;
    Ok(())
}

/// Resolve the linked peer node id when syn and rustdoc inventories were both loaded.
#[instrument(level = "debug", skip(node))]
pub fn syn_doc_peer(node: &dyn NodeView) -> Option<crate::ir::NodeId> {
    let id = node
        .attr(ATTR_SYN_DOC_PEER)
        .and_then(|value| value.as_u64())
        .map(|value| crate::ir::NodeId(value as u32))?;
    Some(id)
}

#[instrument(level = "debug", skip(path))]
pub fn inventory_link_key(path: &str, crate_name: &str) -> String {
    let normalized = crate_name.replace('-', "_");
    if path.split("::").next() == Some(normalized.as_str()) {
        path.to_string()
    } else {
        format!("{normalized}::{path}")
    }
}
