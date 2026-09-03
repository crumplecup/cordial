use crate::enricher::{member_crate_root, resolve_parent};
use crate::error::CordialResult;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{ItemKind, NodeKind, NodeWeight};
use crate::loader::SourceLoadView;
use crate::objects::FileSpan;

use super::call_graph::workspace_call_graph;
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

    #[instrument(level = "trace", skip(self, view))]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let ir = view.ir;
        let load = view.load;
        let session = view.session;

        let Some(source) = load.as_any().downcast_ref::<SourceLoadView>() else {
            return Ok(());
        };

        let crate_root = member_crate_root(source, session);
        let config = crate::config::load_session_config(session);
        let extra_skip = config.tracing().extra_skip();
        let call_graph = workspace_call_graph(session.project_root(), config.tracing());
        let never_instrument = call_graph.never_instrument(source.crate_name());
        let records = scan_source_tree(
            source.src_root(),
            &crate_root,
            source.crate_name(),
            extra_skip,
            never_instrument,
        )?;
        let facts = crate::workspace_path_inclusions(session.project_root());
        let mut policy_by_file: std::collections::HashMap<std::path::PathBuf, &'static str> =
            std::collections::HashMap::new();

        for record in records {
            let file_path = crate_root.join(record.file());
            let policy = *policy_by_file.entry(file_path.clone()).or_insert_with(|| {
                match super::apply::resolve_tracing_apply_policy(
                    source.crate_name(),
                    &file_path,
                    &crate_root,
                    config.tracing(),
                    &facts,
                ) {
                    super::apply::TracingApplyPolicy::Skip => "skip",
                    super::apply::TracingApplyPolicy::Gated(_) => "gated",
                    super::apply::TracingApplyPolicy::Bare => "bare",
                }
            });
            if policy == "skip" && !record.instrumented() {
                continue;
            }

            let parent = resolve_parent(ir, &module_context(record.qualified_name()))?;
            let span = FileSpan::new(crate_root.join(record.file()), record.line(), 1);

            let node = if let Some(existing) = ir.node_by_path(record.qualified_name()) {
                existing
            } else {
                let node = ir.insert_node(
                    NodeWeight::new(NodeKind::Item(ItemKind::Fn))
                        .with_name(
                            record
                                .qualified_name()
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
                serde_json::Value::String(record.qualified_name().clone()),
            )?;
            ir.set_attr(
                node,
                "function_kind",
                serde_json::Value::String(record.kind().to_string()),
            )?;
            ir.set_attr(
                node,
                "function_role",
                serde_json::Value::String(record.role().to_string()),
            )?;
            ir.set_attr(
                node,
                "function_complexity",
                serde_json::Value::String(record.complexity().to_string()),
            )?;
            ir.set_attr(
                node,
                "recipe_level",
                serde_json::Value::String(record.recipe().level().to_string()),
            )?;
            ir.set_attr(
                node,
                "recipe_skip",
                serde_json::Value::String(record.recipe().skip().join(",")),
            )?;
            ir.set_attr(
                node,
                "recipe_fields",
                serde_json::Value::String(record.recipe().fields().join(",")),
            )?;
            ir.set_attr(
                node,
                "recipe_err",
                serde_json::Value::String(
                    record
                        .recipe()
                        .err()
                        .map(|level| level.to_string())
                        .unwrap_or_default(),
                ),
            )?;
            ir.set_attr(
                node,
                "recipe_ret",
                serde_json::Value::Bool(record.recipe().ret()),
            )?;
            ir.set_attr(
                node,
                "visibility",
                serde_json::Value::String(record.visibility().to_string()),
            )?;
            ir.set_attr(
                node,
                "file",
                serde_json::Value::String(record.file().clone()),
            )?;
            ir.set_attr(
                node,
                "line",
                serde_json::Value::Number(record.line().into()),
            )?;
            ir.set_attr(
                node,
                "instrumented",
                serde_json::Value::Bool(record.instrumented()),
            )?;
            ir.set_attr(
                node,
                "has_error_path_event",
                serde_json::Value::Bool(record.has_error_path_event()),
            )?;
            ir.set_attr(
                node,
                "param_names",
                serde_json::Value::String(record.param_names().join(",")),
            )?;
            ir.set_attr(
                node,
                "proof_only",
                serde_json::Value::Bool(record.proof_only()),
            )?;
            ir.set_attr(
                node,
                "prover_visible_instrument",
                serde_json::Value::Bool(record.prover_visible_instrument()),
            )?;
            ir.set_attr(
                node,
                "tracing_apply_policy",
                serde_json::Value::String(policy.to_string()),
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
