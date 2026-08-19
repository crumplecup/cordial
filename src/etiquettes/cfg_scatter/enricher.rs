use crate::enricher::member_crate_root;
use crate::error::CordialResult;
use crate::hooks::IrEnricher;
use crate::ir::{EdgeKind, IrMut, NodeKind, NodeWeight};
use crate::loader::{LoadView, SourceLoadView};
use crate::objects::FileSpan;
use crate::session::SessionView;

use super::scan::scan_source_tree;
use super::types::CfgScatterRecord;

use tracing::instrument;
/// Materializes scattered-`cfg` group nodes in the IR graph.
#[derive(Debug, Default, Clone, Copy)]
pub struct CfgScatterInventoryEnricher;

impl CfgScatterInventoryEnricher {
    pub const ID: &'static str = "cfg-scatter-inventory";
}

impl IrEnricher for CfgScatterInventoryEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
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
        let thresholds = crate::config::load_session_config(session).cfg_scatter;
        let groups = scan_source_tree(&source.src_root, &crate_root, thresholds)?;

        for group in &groups {
            let record = CfgScatterRecord::from(group);
            let file = crate_root.join(&record.file);
            let span = FileSpan::new(file.clone(), 1, 1);
            let node = ir.insert_node(
                NodeWeight::new(NodeKind::Expr)
                    .with_span(span)
                    .with_name(format!("cfg({})", record.predicate)),
            )?;
            ir.set_attr(
                node,
                "cfg_scatter_predicate",
                serde_json::Value::String(record.predicate.clone()),
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
                        .distinct_kinds
                        .iter()
                        .map(|kind| kind.as_str())
                        .collect::<Vec<_>>()
                        .join("+"),
                ),
            )?;
            ir.set_attr(
                node,
                "occurrences",
                serde_json::Value::Number(record.occurrence_count.into()),
            )?;
            ir.set_attr(
                node,
                "sample",
                serde_json::Value::String(record.sample_snippets.join("; ")),
            )?;
            ir.insert_edge(ir.root()?, node, EdgeKind::Contains)?;
        }

        Ok(())
    }
}
