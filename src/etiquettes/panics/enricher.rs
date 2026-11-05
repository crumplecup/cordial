use crate::enricher::resolve_parent;
use crate::error::CordialResult;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{EdgeKind, NodeKind, NodeWeight};
use crate::objects::FileSpan;

use tracing::instrument;
/// Materializes panic-site expression nodes in the IR graph.
#[derive(Debug, Default, Clone, Copy)]
pub struct PanicInventoryEnricher;

impl PanicInventoryEnricher {
    pub const ID: &'static str = "panic-inventory";
}

impl IrEnricher for PanicInventoryEnricher {
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
        let ir = view.ir();
        let inventory = ir.source_panics()?;
        let crate_root = inventory.crate_root();
        let records = inventory.records();

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
                "panic_kind",
                serde_json::Value::String(record.kind().as_attr().to_string()),
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
            ir.set_attr(node, "cfg_test", serde_json::Value::Bool(record.cfg_test()))?;
            ir.insert_edge(parent, node, EdgeKind::Contains)?;
        }

        Ok(())
    }
}
