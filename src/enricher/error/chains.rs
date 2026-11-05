//! Join loader-collected chain records to the graph's located error sites.

use crate::hooks::{EnrichView, IrEnricher};
use crate::{CordialResult, SourceSpan};
use std::collections::HashMap;
use tracing::instrument;

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct ErrorChainInventoryEnricher;

impl ErrorChainInventoryEnricher {
    pub const ID: &'static str = "error-chain-inventory";
}

impl IrEnricher for ErrorChainInventoryEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        50
    }

    #[instrument(level = "trace", skip(self, view), err(level = "warn"))]
    #[hotpath::measure(impl_type = "ErrorChainInventoryEnricher")]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let ir = view.ir();
        let inventory = ir.source_error_chains()?;
        let crate_root = inventory.crate_root();
        let mut site_nodes: HashMap<(String, u32), Vec<crate::ir::NodeId>> = HashMap::new();
        for node in ir.nodes_matching(
            &crate::ir::QueryBuilder::new()
                .has_attr("error_site_kind")
                .build(),
        ) {
            let span = node.weight().span().as_ref().ok_or_else(|| {
                crate::CordialError::workspace_source_facts_missing("located error-site node")
            })?;
            let file = span.file().strip_prefix(crate_root).unwrap_or(span.file());
            site_nodes
                .entry(site_key(file, span.line()))
                .or_default()
                .push(node.id());
        }

        for record in inventory.records() {
            let key = site_key(record.file(), record.line());
            if let Some(nodes) = site_nodes.get(&key) {
                for &node in nodes {
                    chain::apply_chain_attrs(ir, node, &record)?;
                }
            } else {
                chain::insert_error_chain_node(ir, crate_root, &record)?;
            }
        }

        Ok(())
    }
}

#[instrument(level = "debug", skip(file))]
fn site_key(file: &std::path::Path, line: u32) -> (String, u32) {
    (file.display().to_string(), line)
}

/// `error_chain` node materialization, gated as a whole unit — see
/// `docs/planning/cfg-scatter-etiquette.md` for the pattern.
mod chain {
    use crate::enricher::resolve_parent;
    use crate::error::CordialResult;
    use crate::etiquettes::error_chain::ErrorChainRecord;
    use crate::ir::{EdgeKind, IrMut, NodeKind, NodeWeight};
    use crate::objects::FileSpan;
    use tracing::instrument;

    #[instrument(level = "debug", skip(ir, node, record), err(level = "warn"))]
    pub(super) fn apply_chain_attrs(
        ir: &mut dyn IrMut,
        node: crate::ir::NodeId,
        record: &ErrorChainRecord,
    ) -> CordialResult<()> {
        ir.set_attr(
            node,
            "error_chain_rule_id",
            serde_json::Value::String(record.rule_id().as_str().to_string()),
        )?;
        if !ir.node(node).is_some_and(|n| n.attr("context").is_some()) {
            ir.set_attr(
                node,
                "context",
                serde_json::Value::String(record.context().clone()),
            )?;
        }
        ir.set_attr(
            node,
            "snippet",
            serde_json::Value::String(record.snippet().clone()),
        )?;
        ir.set_attr(
            node,
            "foreign_error_type",
            serde_json::Value::String(record.foreign_error_type().clone().unwrap_or_default()),
        )?;
        Ok(())
    }

    #[instrument(level = "debug", skip(ir, record), err(level = "warn"))]
    pub(super) fn insert_error_chain_node(
        ir: &mut dyn IrMut,
        crate_root: &std::path::Path,
        record: &ErrorChainRecord,
    ) -> CordialResult<()> {
        let parent = resolve_parent(ir, record.context())?;
        let file = crate_root.join(record.file());
        let span = FileSpan::new(file.clone(), record.line(), 1);
        let node = ir.insert_node(
            NodeWeight::new(NodeKind::Expr)
                .with_span(span.clone())
                .with_name(record.snippet().clone()),
        )?;
        apply_chain_attrs(ir, node, record)?;
        ir.set_attr(
            node,
            "file",
            serde_json::Value::String(file.display().to_string()),
        )?;
        ir.set_attr(
            node,
            "line",
            serde_json::Value::Number(record.line().into()),
        )?;
        ir.insert_edge(parent, node, EdgeKind::Contains)?;
        Ok(())
    }
}
