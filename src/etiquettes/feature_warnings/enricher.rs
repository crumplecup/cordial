use crate::enricher::{member_crate_root, resolve_parent};
use crate::error::CordialResult;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{EdgeKind, NodeKind, NodeWeight};
use crate::loader::SourceLoadView;
use crate::objects::FileSpan;

use super::scan::scan_crate_feature_warnings;

use tracing::instrument;

/// Materializes feature-dependent warning expression nodes in the IR graph.
#[derive(Debug, Default, Clone, Copy)]
pub struct FeatureWarningInventoryEnricher;

impl FeatureWarningInventoryEnricher {
    /// Stable identifier for `FeatureWarningInventoryEnricher`.
    pub const ID: &'static str = "feature-warning-inventory";
}

impl IrEnricher for FeatureWarningInventoryEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let (ir, load, session) = view.into_parts();

        let Some(source) = load.as_any().downcast_ref::<SourceLoadView>() else {
            return Ok(());
        };

        let crate_root = member_crate_root(source, session);
        let policy = crate::config::load_session_config(session)
            .feature_warnings()
            .clone();
        if !policy.enabled() {
            return Ok(());
        }
        let records = scan_crate_feature_warnings(
            &crate_root,
            session.project_root(),
            ir.crate_name(),
            &policy,
        )?;

        for record in records {
            let parent = resolve_parent(ir, "<crate>")?;
            let span = FileSpan::new(record.file().clone(), record.line(), 1);
            let node = ir.insert_node(
                NodeWeight::new(NodeKind::Expr)
                    .with_span(span)
                    .with_name(record.message().clone()),
            )?;
            let text = |value: &str| serde_json::Value::String(value.to_string());
            ir.set_attr(
                node,
                "feature_warning_rule_id",
                text(record.rule_id().as_str()),
            )?;
            ir.set_attr(node, "lint", text(record.lint()))?;
            ir.set_attr(node, "message", text(record.message()))?;
            ir.set_attr(node, "gate", text(record.gate()))?;
            ir.set_attr(node, "wide", serde_json::Value::Bool(record.wide()))?;
            ir.set_attr(node, "advice", text(record.advice()))?;
            ir.set_attr(node, "triggering", text(record.triggering()))?;
            ir.set_attr(node, "file", text(&record.file().display().to_string()))?;
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
