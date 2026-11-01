use crate::enricher::resolve_parent;
use crate::error::CordialResult;
use crate::hooks::{SourceEnrichView, SourceEnricher};
use crate::ir::{EdgeKind, NodeKind, NodeWeight};
use crate::objects::FileSpan;

use tracing::instrument;
/// Materializes cfg-hygiene-site expression nodes in the IR graph.
#[derive(Debug, Default, Clone, Copy)]
pub struct CfgHygieneInventoryEnricher;

impl CfgHygieneInventoryEnricher {
    /// Stable registration identity for cfg-name policy materialization.
    pub const ID: &'static str = "cfg-hygiene-inventory";
}

impl SourceEnricher for CfgHygieneInventoryEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich_source(&self, view: SourceEnrichView<'_>) -> CordialResult<()> {
        let (ir, _source, session) = view.into_parts();
        let inventory = ir
            .source_policy_inventory()?
            .ok_or_else(|| crate::CordialError::not_found("source policy inventory".into()))?;

        let crate_root = inventory.crate_root();
        let thresholds = crate::config::load_session_config(session)
            .cfg_hygiene()
            .clone();
        let records =
            super::scan_crate::records_for_inventory(&inventory, ir.crate_name(), &thresholds)?;

        for record in records {
            let parent = resolve_parent(ir, record.context())?;
            let file = crate_root.join(record.file());
            let span = FileSpan::new(file.clone(), record.line(), 1);
            let node = ir.insert_node(
                NodeWeight::new(NodeKind::Expr)
                    .with_span(span)
                    .with_name(format!("cfg({})", record.cfg_name())),
            )?;
            ir.set_attr(
                node,
                "cfg_hygiene_rule_id",
                serde_json::Value::String(record.rule_id().as_str().to_string()),
            )?;
            ir.set_attr(
                node,
                "cfg_name",
                serde_json::Value::String(record.cfg_name().clone()),
            )?;
            ir.set_attr(
                node,
                "context",
                serde_json::Value::String(record.context().clone()),
            )?;
            ir.set_attr(
                node,
                "snippet",
                serde_json::Value::String(record.snippet().clone()),
            )?;
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
        }

        Ok(())
    }
}
