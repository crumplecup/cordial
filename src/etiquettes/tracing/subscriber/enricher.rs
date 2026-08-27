use crate::enricher::{member_crate_root, resolve_parent};
use crate::error::CordialResult;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{EdgeKind, NodeKind, NodeWeight};
use crate::loader::SourceLoadView;
use crate::objects::FileSpan;

use super::scan::scan_crate_tracing_subscriber;

use tracing::instrument;

/// Materializes tracing-subscriber policy expression nodes in the IR graph.
#[derive(Debug, Default, Clone, Copy)]
pub struct SubscriberInventoryEnricher;

impl SubscriberInventoryEnricher {
    pub const ID: &'static str = "tracing-subscriber-inventory";
}

impl IrEnricher for SubscriberInventoryEnricher {
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
        let config = crate::config::load_session_config(session);
        let skip_program_lints = crate_skips_program_lints(&source.crate_name, config.tracing());
        let records = scan_crate_tracing_subscriber(
            &crate_root,
            &source.crate_name,
            config.tracing().subscriber(),
            skip_program_lints,
        )?;

        for record in records {
            let parent = resolve_parent(ir, "<crate>")?;
            let file = crate_root.join(&record.file);
            let span = FileSpan::new(file.clone(), record.line, 1);
            let node = ir.insert_node(
                NodeWeight::new(NodeKind::Expr)
                    .with_span(span)
                    .with_name(record.snippet.clone()),
            )?;
            ir.set_attr(
                node,
                "subscriber_rule_id",
                serde_json::Value::String(record.rule_id.as_str().to_string()),
            )?;
            ir.set_attr(node, "context", serde_json::Value::String(record.context))?;
            ir.set_attr(node, "snippet", serde_json::Value::String(record.snippet))?;
            ir.set_attr(
                node,
                "file",
                serde_json::Value::String(file.display().to_string()),
            )?;
            ir.set_attr(node, "line", serde_json::Value::Number(record.line.into()))?;
            ir.insert_edge(parent, node, EdgeKind::Contains)?;
        }

        Ok(())
    }
}

/// MAIN/TEST are for logging programs. Skip/gate crates are named in
/// `[tracing]` as verifier targets, not as ordinary binaries/tests.
#[instrument(level = "debug", skip(config))]
fn crate_skips_program_lints(crate_name: &str, config: &crate::config::TracingThresholds) -> bool {
    config
        .apply_skip_crates()
        .iter()
        .any(|skip| skip == crate_name)
        || config.apply_gate_crates().contains_key(crate_name)
}
