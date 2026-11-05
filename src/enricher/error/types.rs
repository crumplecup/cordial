//! Materialize loader-owned error relationships without source access.

use crate::CordialResult;
use crate::enricher::resolve_parent;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{EdgeKind, NodeKind, NodeWeight};
use crate::objects::FileSpan;
use tracing::instrument;

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct ErrorTypeInventoryEnricher;

pub(super) static ERROR_TYPE_INVENTORY: ErrorTypeInventoryEnricher = ErrorTypeInventoryEnricher;

impl ErrorTypeInventoryEnricher {
    pub const ID: &'static str = "error-type-inventory";
}

impl IrEnricher for ErrorTypeInventoryEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        50
    }

    #[instrument(level = "trace", skip(self, view), err(level = "warn"))]
    #[hotpath::measure(impl_type = "ErrorTypeInventoryEnricher")]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let ir = view.ir();
        let inventory = ir.source_error_types()?;
        let crate_root = inventory.crate_root();
        let report = inventory.report(ir.crate_name(), true)?;
        for node in report.nodes() {
            let parent = resolve_parent(ir, node.type_path())?;
            let file = crate_root.join(node.file());
            let span = FileSpan::new(file.clone(), node.line(), 1);
            let ir_node = ir.insert_node(
                NodeWeight::new(NodeKind::Expr)
                    .with_span(span.clone())
                    .with_name(node.snippet().clone()),
            )?;
            ir.set_attr(
                ir_node,
                "internal_error_record_kind",
                serde_json::Value::String("type_graph".to_string()),
            )?;
            ir.set_attr(
                ir_node,
                "type_path",
                serde_json::Value::String(node.type_path().clone()),
            )?;
            ir.set_attr(
                ir_node,
                "node_class",
                serde_json::Value::String(node.node_class().to_string()),
            )?;
            ir.set_attr(
                ir_node,
                "probe_id",
                serde_json::Value::String(node.probe_id().to_string()),
            )?;
            ir.set_attr(
                ir_node,
                "source_target",
                serde_json::Value::String(node.source_target().clone().unwrap_or_default()),
            )?;
            ir.set_attr(
                ir_node,
                "reaches_foreign",
                serde_json::Value::Bool(node.reaches_foreign()),
            )?;
            ir.set_attr(
                ir_node,
                "chain_depth",
                serde_json::Value::Number(node.chain_depth().into()),
            )?;
            ir.set_attr(
                ir_node,
                "snippet",
                serde_json::Value::String(node.snippet().clone()),
            )?;
            ir.set_attr(
                ir_node,
                "file",
                serde_json::Value::String(file.display().to_string()),
            )?;
            ir.set_attr(
                ir_node,
                "line",
                serde_json::Value::Number(node.line().into()),
            )?;
            ir.insert_edge(parent, ir_node, EdgeKind::Contains)?;
        }

        Ok(())
    }
}
