use crate::enricher::member_crate_root;
use crate::error::CordialResult;
use crate::hooks::IrEnricher;
use crate::ir::{EdgeKind, IrMut, NodeKind, NodeWeight};
use crate::loader::{LoadView, SourceLoadView};
use crate::objects::FileSpan;
use crate::session::SessionView;

use super::scan::scan_crate_cli_layout;

use tracing::instrument;
/// Materializes CLI-layout findings as IR nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct CliLayoutInventoryEnricher;

impl CliLayoutInventoryEnricher {
    pub const ID: &'static str = "cli-layout-inventory";
}

impl IrEnricher for CliLayoutInventoryEnricher {
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
        let records = scan_crate_cli_layout(&crate_root, ir.crate_name())?;

        for record in records {
            let file = if record.file.is_absolute() {
                record.file.clone()
            } else {
                crate_root.join(&record.file)
            };
            let span = FileSpan::new(file.clone(), record.line, 1);
            let node = ir.insert_node(
                NodeWeight::new(NodeKind::Expr)
                    .with_span(span)
                    .with_name(format!("{} {}", record.rule_id, record.context)),
            )?;
            ir.set_attr(
                node,
                "cli_layout_rule",
                serde_json::Value::String(record.rule_id.as_str().to_string()),
            )?;
            ir.set_attr(
                node,
                "file",
                serde_json::Value::String(file.display().to_string()),
            )?;
            ir.set_attr(node, "line", serde_json::Value::Number(record.line.into()))?;
            ir.set_attr(
                node,
                "context",
                serde_json::Value::String(record.context.clone()),
            )?;
            ir.set_attr(
                node,
                "snippet",
                serde_json::Value::String(record.snippet.clone()),
            )?;
            ir.insert_edge(ir.root()?, node, EdgeKind::Contains)?;
        }

        Ok(())
    }
}
