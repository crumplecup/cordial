use crate::enricher::resolve_parent;
use crate::error::CordialResult;
use crate::hooks::{SourceEnrichView, SourceEnricher};
use crate::ir::{EdgeKind, NodeKind, NodeWeight};
use crate::objects::FileSpan;

use super::scan::findings_from_inventory;

use tracing::instrument;

/// Materializes leftover-print expression nodes in the IR graph.
#[derive(Debug, Default, Clone, Copy)]
pub struct PrintInventoryEnricher;

impl PrintInventoryEnricher {
    pub const ID: &'static str = "tracing-print-inventory";
}

impl SourceEnricher for PrintInventoryEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich_source(&self, view: SourceEnrichView<'_>) -> CordialResult<()> {
        let (ir, _source, session) = view.into_parts();

        let inventory = ir.source_observability()?;
        let crate_root = inventory.crate_root();
        let config = crate::config::load_session_config(session);
        let records = findings_from_inventory(&inventory, config.tracing().stdio())?;

        for record in records {
            let parent = resolve_parent(ir, record.context())?;
            let file = crate_root.join(record.file());
            let span = FileSpan::new(file.clone(), record.line(), 1);
            let node = ir.insert_node(
                NodeWeight::new(NodeKind::Expr)
                    .with_span(span)
                    .with_name(record.snippet().clone()),
            )?;
            ir.set_attr(
                node,
                "print_rule_id",
                serde_json::Value::String(record.rule_id().as_str().to_string()),
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
