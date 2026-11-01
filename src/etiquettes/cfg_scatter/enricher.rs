use crate::error::CordialResult;
use crate::hooks::{SourceEnrichView, SourceEnricher};
use crate::ir::{EdgeKind, NodeKind, NodeWeight};
use crate::objects::FileSpan;

use super::types::CfgScatterRecord;

use tracing::instrument;
/// Materializes scattered-`cfg` group nodes in the IR graph.
#[derive(Debug, Default, Clone, Copy)]
pub struct CfgScatterInventoryEnricher;

impl CfgScatterInventoryEnricher {
    /// Stable registration identity for cfg-scatter materialization.
    pub const ID: &'static str = "cfg-scatter-inventory";
}

impl SourceEnricher for CfgScatterInventoryEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich_source(&self, view: SourceEnrichView<'_>) -> CordialResult<()> {
        let (ir, _source, session) = view.into_parts();
        let inventory = ir
            .source_policy_inventory()?
            .ok_or_else(|| crate::CordialError::not_found("source policy inventory".into()))?;

        let crate_root = inventory.crate_root();
        let thresholds = *crate::config::load_session_config(session).cfg_scatter();
        let groups = inventory
            .cfg_groups()
            .iter()
            .filter(|group| group.is_scatter(&thresholds));

        for group in groups {
            let record = CfgScatterRecord::from_group(group)?;
            let file = crate_root.join(record.file());
            let span = FileSpan::new(file.clone(), 1, 1);
            let node = ir.insert_node(
                NodeWeight::new(NodeKind::Expr)
                    .with_span(span)
                    .with_name(format!("cfg({})", record.predicate())),
            )?;
            ir.set_attr(
                node,
                "cfg_scatter_predicate",
                serde_json::Value::String(record.predicate().clone()),
            )?;
            ir.set_attr(
                node,
                "file",
                serde_json::Value::String(file.display().to_string()),
            )?;
            ir.set_attr(
                node,
                "kinds",
                serde_json::Value::String(
                    record
                        .distinct_kinds()
                        .iter()
                        .map(|kind| kind.as_str())
                        .collect::<Vec<_>>()
                        .join("+"),
                ),
            )?;
            ir.set_attr(
                node,
                "occurrences",
                serde_json::Value::Number(record.occurrence_count().into()),
            )?;
            ir.set_attr(
                node,
                "sample",
                serde_json::Value::String(record.sample_snippets().join("; ")),
            )?;
            ir.insert_edge(ir.root()?, node, EdgeKind::Contains)?;
        }

        Ok(())
    }
}
