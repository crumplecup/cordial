use crate::enricher::{member_crate_root, resolve_parent};
use crate::error::CordialResult;
use crate::hooks::IrEnricher;
use crate::ir::{EdgeKind, IrMut, NodeKind, NodeWeight};
use crate::loader::{LoadView, SourceLoadView};
use crate::objects::FileSpan;
use crate::session::SessionView;

use super::scan::scan_source_tree;

/// Materializes derive-pattern expression nodes in the IR graph.
#[derive(Debug, Default, Clone, Copy)]
pub struct DeriveInventoryEnricher;

impl DeriveInventoryEnricher {
    pub const ID: &'static str = "derive-inventory";
}

impl IrEnricher for DeriveInventoryEnricher {
    fn id(&self) -> &str {
        Self::ID
    }

    fn enrich(
        &self,
        ir: &mut dyn IrMut,
        load: &dyn LoadView,
        session: &dyn SessionView,
    ) -> CordialResult<()> {
        let Some(source) = load.as_any().downcast_ref::<SourceLoadView>() else {
            return Ok(());
        };

        let crate_root = member_crate_root(source, session);
        let records = scan_source_tree(&source.src_root, &crate_root)?;

        for record in records {
            let parent = resolve_parent(ir, &record.qualified_name)?;
            let file = crate_root.join(&record.file);
            let span = FileSpan::new(file.clone(), record.line, 1);
            let node = ir.insert_node(
                NodeWeight::new(NodeKind::Expr)
                    .with_span(span.clone())
                    .with_name(record.qualified_name.clone()),
            )?;
            ir.set_attr(
                node,
                "derive_rule_id",
                serde_json::Value::String(record.rule_id.as_str().to_string()),
            )?;
            ir.set_attr(
                node,
                "struct_name",
                serde_json::Value::String(record.struct_name.clone()),
            )?;
            if let Some(method_name) = &record.method_name {
                ir.set_attr(
                    node,
                    "method_name",
                    serde_json::Value::String(method_name.clone()),
                )?;
            }
            ir.set_attr(
                node,
                "qualified_name",
                serde_json::Value::String(record.qualified_name.clone()),
            )?;
            ir.set_attr(
                node,
                "recommendation",
                serde_json::Value::String(record.recommendation.clone()),
            )?;
            ir.set_attr(
                node,
                "file",
                serde_json::Value::String(file.display().to_string()),
            )?;
            ir.set_attr(node, "line", serde_json::Value::Number(record.line.into()))?;
            ir.set_attr(
                node,
                "evidence",
                serde_json::Value::String(record.evidence.clone()),
            )?;
            ir.insert_edge(parent, node, EdgeKind::Contains)?;
        }

        Ok(())
    }
}
