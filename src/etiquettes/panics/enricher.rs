use crate::enricher::{member_crate_root, resolve_parent};
use crate::error::CordialResult;
use crate::hooks::{SourceEnrichView, SourceEnricher};
use crate::ir::{EdgeKind, NodeKind, NodeWeight};
use crate::objects::FileSpan;

use super::scan::scan_crate_panics_with_source;

use tracing::instrument;
/// Materializes panic-site expression nodes in the IR graph.
#[derive(Debug, Default, Clone, Copy)]
pub struct PanicInventoryEnricher;

impl PanicInventoryEnricher {
    pub const ID: &'static str = "panic-inventory";
}

impl SourceEnricher for PanicInventoryEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        2
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich_source(&self, view: SourceEnrichView<'_>) -> CordialResult<()> {
        let (ir, source, session) = view.into_parts();

        let crate_root = member_crate_root(source, session);
        #[cfg(feature = "verus_ir")]
        let verus = ir.verus_facts().ok_or_else(|| {
            crate::error::CordialError::workspace_source_facts_missing("Verus macro facts")
        })?;
        let records = scan_crate_panics_with_source(
            &crate_root,
            source,
            #[cfg(feature = "verus_ir")]
            &verus,
        )?;

        for record in records {
            let parent = resolve_parent(ir, record.context())?;
            let file = crate_root.join(record.file());
            let span = FileSpan::new(file.clone(), record.line(), 1);
            let node = ir.insert_node(
                NodeWeight::new(NodeKind::Expr)
                    .with_span(span.clone())
                    .with_name(record.snippet().clone()),
            )?;
            ir.set_attr(
                node,
                "panic_kind",
                serde_json::Value::String(record.kind().as_attr().to_string()),
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
            ir.set_attr(node, "cfg_test", serde_json::Value::Bool(record.cfg_test()))?;
            ir.insert_edge(parent, node, EdgeKind::Contains)?;
        }

        Ok(())
    }
}
