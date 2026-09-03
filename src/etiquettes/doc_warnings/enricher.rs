use crate::enricher::{member_crate_root, resolve_parent};
use crate::error::CordialResult;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{EdgeKind, NodeKind, NodeWeight};
use crate::loader::SourceLoadView;
use crate::objects::FileSpan;

use super::scan::scan_crate_doc_warnings;

use tracing::instrument;

/// Materializes rustdoc-warning expression nodes in the IR graph.
#[derive(Debug, Default, Clone, Copy)]
pub struct DocWarningInventoryEnricher;

impl DocWarningInventoryEnricher {
    /// Stable identifier for `DocWarningInventoryEnricher`.
    pub const ID: &'static str = "doc-warning-inventory";
}

impl IrEnricher for DocWarningInventoryEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
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
        let policy = crate::config::load_session_config(session)
            .doc_warnings()
            .clone();
        let records = scan_crate_doc_warnings(
            &crate_root,
            session.project_root(),
            ir.crate_name(),
            &policy,
        )?;

        for record in records {
            let parent = resolve_parent(ir, "<crate>")?;
            let file = if record.file().is_absolute() {
                record.file().clone()
            } else {
                crate_root.join(record.file())
            };
            let span = FileSpan::new(file.clone(), record.line(), 1);
            let node = ir.insert_node(
                NodeWeight::new(NodeKind::Expr)
                    .with_span(span)
                    .with_name(record.snippet().clone()),
            )?;
            ir.set_attr(
                node,
                "doc_warning_rule_id",
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
