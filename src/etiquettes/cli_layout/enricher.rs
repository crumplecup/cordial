use crate::enricher::member_crate_root;
use crate::error::CordialResult;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{EdgeKind, NodeKind, NodeWeight};
use crate::loader::SourceLoadView;
use crate::objects::FileSpan;

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

    #[instrument(level = "trace", skip(self, view))]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let ir = view.ir;
        let load = view.load;
        let session = view.session;

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
