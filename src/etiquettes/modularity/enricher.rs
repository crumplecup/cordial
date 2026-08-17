use crate::enricher::{member_crate_root, resolve_parent};
use crate::error::CordialResult;
use crate::hooks::IrEnricher;
use crate::ir::{EdgeKind, IrMut, NodeKind, NodeWeight};
use crate::loader::{LoadView, SourceLoadView};
use crate::objects::FileSpan;
use crate::session::SessionView;

use super::scan::scan_source_tree;
use super::types::ModularityKind;

/// Materializes modularity-site expression nodes in the IR graph.
#[derive(Debug, Default, Clone, Copy)]
pub struct ModularityInventoryEnricher;

impl ModularityInventoryEnricher {
    pub const ID: &'static str = "modularity-inventory";
}

impl IrEnricher for ModularityInventoryEnricher {
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
        let thresholds = crate::config::load_session_config(session).modularity;
        let records = scan_source_tree(&source.src_root, &crate_root, thresholds)?;

        for record in records {
            let parent = match record.kind {
                ModularityKind::Function | ModularityKind::ModuleSize
                    if !record.context.is_empty() =>
                {
                    resolve_parent(ir, &record.context)?
                }
                _ => ir.root()?,
            };
            let file = crate_root.join(&record.file);
            let span = FileSpan::new(file.clone(), record.line, 1);
            let label = if record.context.is_empty() {
                record.file.display().to_string()
            } else {
                record.context.clone()
            };
            let node = ir.insert_node(
                NodeWeight::new(NodeKind::Expr)
                    .with_span(span.clone())
                    .with_name(label),
            )?;
            ir.set_attr(
                node,
                "modularity_kind",
                serde_json::Value::String(record.kind.as_str().to_string()),
            )?;
            ir.set_attr(node, "context", serde_json::Value::String(record.context))?;
            ir.set_attr(
                node,
                "file",
                serde_json::Value::String(file.display().to_string()),
            )?;
            ir.set_attr(node, "line", serde_json::Value::Number(record.line.into()))?;
            ir.set_attr(
                node,
                "lines",
                serde_json::Value::Number(record.lines.into()),
            )?;
            if record.kind == ModularityKind::ModuleSize {
                ir.set_attr(node, "inline", serde_json::Value::Bool(record.inline))?;
            }
            ir.insert_edge(parent, node, EdgeKind::Contains)?;
        }

        Ok(())
    }
}
