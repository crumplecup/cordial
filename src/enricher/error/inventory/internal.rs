//! Remaining compatibility collection for internal architecture and apply.

use crate::enricher::error::scan::chain::scan_crate_error_ir_with_source;
use crate::enricher::member_crate_root;
use crate::enricher::resolve_parent;
use crate::error::CordialResult;
use crate::hooks::SourceEnrichView;
use crate::ir::IrMut;
use crate::ir::{EdgeKind, NodeKind, NodeWeight};
use crate::objects::FileSpan;
use tracing::instrument;

#[instrument(level = "debug", skip(view), err(level = "warn"))]
pub(super) fn enrich(view: SourceEnrichView<'_>) -> CordialResult<()> {
    let (ir, source, session) = view.into_parts();

    let crate_root = member_crate_root(source, session);
    let error_config = crate::config::load_session_config(session);
    let kind_fanout_max_call_sites = error_config
        .internal_error_chain()
        .kind_fanout_max_call_sites();
    let field_strategy = error_config.internal_error_chain().field_strategy();
    let report = scan_crate_error_ir_with_source(
        &crate_root,
        ir.crate_name(),
        source,
        kind_fanout_max_call_sites,
        field_strategy,
    )?;
    materialize_error_ir(ir, &crate_root, &report)?;
    if error_config.etiquette_may_run("internal_error_chain") {
        let mut files = Vec::new();
        crate::etiquettes::internal_error_chain::for_each_shared_src_rust_file(
            source,
            &crate_root,
            |path, _| {
                files.push(source.parsed_file(path, "error-conversion-ir")?);
                Ok(())
            },
        )?;
        crate::etiquettes::internal_error_chain::enrich_error_apply_evidence(
            ir,
            &files,
            field_strategy,
        )?;
    }
    Ok(())
}

#[instrument(level = "debug", skip(ir, report), err(level = "warn"))]
fn materialize_error_ir(
    ir: &mut dyn IrMut,
    crate_root: &std::path::Path,
    report: &crate::enricher::error::scan::ErrorIrScanReport,
) -> CordialResult<()> {
    {
        for finding in report.compliance() {
            let parent = resolve_parent(ir, finding.context())?;
            let file = crate_root.join(finding.file());
            let span = FileSpan::new(file.clone(), finding.line(), 1);
            let ir_node = ir.insert_node(
                NodeWeight::new(NodeKind::Expr)
                    .with_span(span.clone())
                    .with_name(finding.snippet().clone()),
            )?;
            ir.set_attr(
                ir_node,
                "internal_error_record_kind",
                serde_json::Value::String("compliance".to_string()),
            )?;
            ir.set_attr(
                ir_node,
                "rule_id",
                serde_json::Value::String(finding.rule_id().to_string()),
            )?;
            ir.set_attr(
                ir_node,
                "context",
                serde_json::Value::String(finding.context().clone()),
            )?;
            ir.set_attr(
                ir_node,
                "foreign_error_type",
                serde_json::Value::String(finding.foreign_error_type().clone().unwrap_or_default()),
            )?;
            ir.set_attr(
                ir_node,
                "internal_constructor",
                serde_json::Value::String(
                    finding.internal_constructor().clone().unwrap_or_default(),
                ),
            )?;
            ir.set_attr(
                ir_node,
                "snippet",
                serde_json::Value::String(finding.snippet().clone()),
            )?;
            ir.set_attr(
                ir_node,
                "file",
                serde_json::Value::String(file.display().to_string()),
            )?;
            ir.set_attr(
                ir_node,
                "line",
                serde_json::Value::Number(finding.line().into()),
            )?;
            ir.insert_edge(parent, ir_node, EdgeKind::Contains)?;
        }
    }

    Ok(())
}
