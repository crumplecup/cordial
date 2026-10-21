use crate::enricher::{member_crate_root, resolve_parent};
use crate::error::CordialResult;
use crate::hooks::{SourceEnrichView, SourceEnricher};
use crate::ir::{EdgeKind, NodeKind, NodeWeight};
use crate::objects::FileSpan;

use super::scan::scan_crate_verus_warnings_with_source;

use tracing::instrument;

/// Materializes Verus compiler-warning expression nodes in the IR graph.
#[derive(Debug, Default, Clone, Copy)]
pub struct VerusWarningInventoryEnricher;

impl VerusWarningInventoryEnricher {
    pub const ID: &'static str = "verus-warning-inventory";
}

impl SourceEnricher for VerusWarningInventoryEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich_source(&self, view: SourceEnrichView<'_>) -> CordialResult<()> {
        let (ir, source, session) = view.into_parts();

        let crate_root = member_crate_root(source, session);
        let records = scan_crate_verus_warnings_with_source(&crate_root, source)?;

        for record in records {
            let parent = resolve_parent(ir, record.context())?;
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
                "verus_warning_rule_id",
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
