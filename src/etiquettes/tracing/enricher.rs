use crate::enricher::{member_crate_root, resolve_parent};
use crate::error::CordialResult;
use crate::hooks::IrEnricher;
use crate::ir::{IrMut, ItemKind, NodeKind, NodeWeight};
use crate::loader::{LoadView, SourceLoadView};
use crate::objects::FileSpan;
use crate::session::SessionView;

use super::scan::scan_source_tree;

use tracing::instrument;
/// Materializes function item nodes (including impl methods) in the IR graph.
#[derive(Debug, Default, Clone, Copy)]
pub struct FunctionInventoryEnricher;

impl FunctionInventoryEnricher {
    pub const ID: &'static str = "function-inventory";
}

impl IrEnricher for FunctionInventoryEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        1
    }

    #[instrument(level = "trace", skip(self, ir, load, session))]
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
        let extra_skip = crate::config::load_session_config(session)
            .tracing
            .extra_skip;
        let records = scan_source_tree(
            &source.src_root,
            &crate_root,
            &source.crate_name,
            &extra_skip,
        )?;

        for record in records {
            let parent = resolve_parent(ir, &module_context(&record.qualified_name))?;
            let span = FileSpan::new(crate_root.join(&record.file), record.line, 1);

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
                        .with_span(span),
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
                "function_role",
                serde_json::Value::String(record.role.to_string()),
            )?;
            ir.set_attr(
                node,
                "function_complexity",
                serde_json::Value::String(record.complexity.to_string()),
            )?;
            ir.set_attr(
                node,
                "recipe_level",
                serde_json::Value::String(record.recipe.level.to_string()),
            )?;
            ir.set_attr(
                node,
                "recipe_skip",
                serde_json::Value::String(record.recipe.skip.join(",")),
            )?;
            ir.set_attr(
                node,
                "recipe_fields",
                serde_json::Value::String(record.recipe.fields.join(",")),
            )?;
            ir.set_attr(
                node,
                "recipe_err",
                serde_json::Value::String(
                    record
                        .recipe
                        .err
                        .map(|level| level.to_string())
                        .unwrap_or_default(),
                ),
            )?;
            ir.set_attr(
                node,
                "recipe_ret",
                serde_json::Value::Bool(record.recipe.ret),
            )?;
            ir.set_attr(
                node,
                "visibility",
                serde_json::Value::String(record.visibility.to_string()),
            )?;
            ir.set_attr(node, "file", serde_json::Value::String(record.file.clone()))?;
            ir.set_attr(node, "line", serde_json::Value::Number(record.line.into()))?;
            ir.set_attr(
                node,
                "instrumented",
                serde_json::Value::Bool(record.instrumented),
            )?;
            ir.set_attr(
                node,
                "has_error_path_event",
                serde_json::Value::Bool(record.has_error_path_event),
            )?;
            ir.set_attr(
                node,
                "param_names",
                serde_json::Value::String(record.param_names.join(",")),
            )?;
        }
        Ok(())
    }
}

#[instrument(level = "debug")]
fn module_context(qualified_name: &str) -> String {
    match qualified_name.rsplit_once("::") {
        Some((module, _)) => module.to_string(),
        None => "<crate>".to_string(),
    }
}
