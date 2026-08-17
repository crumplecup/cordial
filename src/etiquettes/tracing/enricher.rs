use crate::enricher::{member_crate_root, resolve_parent};
use crate::error::CordialResult;
use crate::hooks::IrEnricher;
use crate::ir::{IrMut, ItemKind, NodeKind, NodeWeight};
use crate::loader::{LoadView, SourceLoadView};
use crate::objects::FileSpan;
use crate::session::SessionView;

use super::scan::scan_source_tree;

/// Materializes function item nodes (including impl methods) in the IR graph.
#[derive(Debug, Default, Clone, Copy)]
pub struct FunctionInventoryEnricher;

impl FunctionInventoryEnricher {
    pub const ID: &'static str = "function-inventory";
}

impl IrEnricher for FunctionInventoryEnricher {
    fn id(&self) -> &str {
        Self::ID
    }

    fn priority(&self) -> u8 {
        1
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
        let records = scan_source_tree(&source.src_root, &crate_root, &source.crate_name)?;

        for record in records {
            let parent = resolve_parent(ir, &module_context(&record.qualified_name))?;
            let file = crate_root.join(&record.file);
            let span = FileSpan::new(file.clone(), record.line, 1);

            let node = if let Some(existing) = ir.node_by_path(&record.qualified_name) {
                existing
            } else {
                let node = ir.insert_node(
                    NodeWeight::new(NodeKind::Item(ItemKind::Fn))
                        .with_name(
                            record
                                .qualified_name
                                .rsplit("::")
                                .next()
                                .unwrap_or("fn")
                                .to_string(),
                        )
                        .with_span(span.clone()),
                )?;
                ir.insert_edge(parent, node, crate::ir::EdgeKind::Contains)?;
                node
            };

            ir.set_attr(
                node,
                "qualified_path",
                serde_json::Value::String(record.qualified_name.clone()),
            )?;
            ir.set_attr(
                node,
                "function_kind",
                serde_json::Value::String(record.kind.to_string()),
            )?;
            ir.set_attr(
                node,
                "visibility",
                serde_json::Value::String(record.visibility.to_string()),
            )?;
            ir.set_attr(
                node,
                "file",
                serde_json::Value::String(file.display().to_string()),
            )?;
            ir.set_attr(node, "line", serde_json::Value::Number(record.line.into()))?;
            ir.set_attr(
                node,
                "instrumented",
                serde_json::Value::Bool(record.instrumented),
            )?;
        }
        Ok(())
    }
}

fn module_context(qualified_name: &str) -> String {
    match qualified_name.rsplit_once("::") {
        Some((module, _)) => module.to_string(),
        None => "<crate>".to_string(),
    }
}
