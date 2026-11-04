//! Project loader-owned error-site evidence into ordinary expression nodes.

use crate::CordialResult;
use crate::enricher::resolve_parent;
use crate::etiquettes::error_sites::ErrorSiteRecord;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{EdgeKind, IrMut, NodeKind, NodeWeight};
use crate::objects::FileSpan;
use tracing::instrument;

/// Materializes the site census using only the shared graph.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct ErrorSiteInventoryEnricher;

impl ErrorSiteInventoryEnricher {
    /// Stable hook identity for the graph-only site projection.
    pub const ID: &'static str = "error-site-inventory";
}

impl IrEnricher for ErrorSiteInventoryEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        49
    }

    #[instrument(level = "trace", skip(self, view), err(level = "warn"))]
    #[hotpath::measure(impl_type = "ErrorSiteInventoryEnricher")]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let ir = view.ir();
        let inventory = ir.source_error_sites()?;
        for record in inventory.records() {
            insert_error_site_node(ir, inventory.crate_root(), &record)?;
        }
        Ok(())
    }
}

#[instrument(level = "debug", skip(ir, record), err(level = "warn"))]
fn insert_error_site_node(
    ir: &mut dyn IrMut,
    crate_root: &std::path::Path,
    record: &ErrorSiteRecord,
) -> CordialResult<crate::ir::NodeId> {
    let parent = resolve_parent(ir, record.context())?;
    let file = crate_root.join(record.file());
    let span = FileSpan::new(file.clone(), record.line(), 1);
    let node = ir.insert_node(
        NodeWeight::new(NodeKind::Expr)
            .with_span(span.clone())
            .with_name(record.site_snippet().clone()),
    )?;
    ir.set_attr(
        node,
        "error_site_kind",
        serde_json::Value::String(record.kind().as_attr().to_string()),
    )?;
    ir.set_attr(
        node,
        "context",
        serde_json::Value::String(record.context().clone()),
    )?;
    ir.set_attr(
        node,
        "source_snippet",
        serde_json::Value::String(record.source_snippet().clone()),
    )?;
    ir.set_attr(
        node,
        "site_snippet",
        serde_json::Value::String(record.site_snippet().clone()),
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
    Ok(node)
}
