use crate::enricher::{member_crate_root, resolve_parent};
use crate::error::CordialResult;
use crate::hooks::IrEnricher;
use crate::ir::{EdgeKind, IrMut, NodeKind, NodeWeight};
use crate::loader::{LoadView, SourceLoadView};
use crate::objects::FileSpan;
use crate::session::SessionView;

use super::scan_crate::scan_crate_antipatterns;

/// Materializes antipattern-site expression nodes in the IR graph.
#[derive(Debug, Default, Clone, Copy)]
pub struct AntipatternInventoryEnricher;

impl AntipatternInventoryEnricher {
    pub const ID: &'static str = "antipattern-inventory";
}

impl IrEnricher for AntipatternInventoryEnricher {
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
        let records = scan_crate_antipatterns(
            &crate_root,
            ir.crate_name(),
            session.project_root(),
            session.store_root(),
        )?;

        for record in records {
            let parent = resolve_parent(ir, &record.context)?;
            let file = crate_root.join(&record.file);
            let span = FileSpan::new(file.clone(), record.line, 1);
            let node = ir.insert_node(
                NodeWeight::new(NodeKind::Expr)
                    .with_span(span.clone())
                    .with_name(record.snippet.clone()),
            )?;
            ir.set_attr(
                node,
                "antipattern_rule_id",
                serde_json::Value::String(record.rule_id.as_str().to_string()),
            )?;
            ir.set_attr(node, "context", serde_json::Value::String(record.context))?;
            ir.set_attr(node, "snippet", serde_json::Value::String(record.snippet))?;
            ir.set_attr(
                node,
                "file",
                serde_json::Value::String(file.display().to_string()),
            )?;
            ir.set_attr(node, "line", serde_json::Value::Number(record.line.into()))?;
            ir.insert_edge(parent, node, EdgeKind::Contains)?;
        }

        Ok(())
    }
}
