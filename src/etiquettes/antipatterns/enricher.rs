use crate::enricher::resolve_parent;
use crate::error::CordialResult;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{EdgeKind, NodeKind, NodeWeight};
use crate::objects::FileSpan;

use tracing::instrument;
/// Materializes antipattern-site expression nodes in the IR graph.
#[derive(Debug, Default, Clone, Copy)]
pub struct AntipatternInventoryEnricher;

impl AntipatternInventoryEnricher {
    pub const ID: &'static str = "antipattern-inventory";
}

impl IrEnricher for AntipatternInventoryEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let (ir, _load, session) = view.into_parts();
        let inventory = ir.source_antipatterns()?;
        let crate_root = inventory.crate_root();
        let config = crate::load_session_config(session);
        let facts = ir.workspace_antipattern_facts().ok_or_else(|| {
            crate::error::CordialError::workspace_source_facts_missing("antipatterns")
        })?;
        let records = inventory.assess(
            ir.crate_name(),
            &facts,
            config.antipatterns().static_refs().strategy(),
            config.internal_error_chain().field_strategy(),
        )?;

        for record in records {
            let parent = resolve_parent(ir, record.context())?;
            let file = crate_root.join(record.file());
            let span = FileSpan::new(file.clone(), record.line(), 1);
            let node = ir.insert_node(
                NodeWeight::new(NodeKind::Expr)
                    .with_span(span.clone())
                    .with_name(record.snippet().clone()),
            )?;
            ir.set_attr(
                node,
                "antipattern_rule_id",
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
