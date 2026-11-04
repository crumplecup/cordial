//! Compatibility chain collection and joins to loader-owned error-site nodes.

use crate::enricher::error::scan::scan_crate_error_ir_with_source;
use crate::enricher::member_crate_root;
#[cfg(feature = "internal_error_chain")]
use crate::enricher::resolve_parent;
use crate::error::CordialResult;
use crate::hooks::SourceEnrichView;
use crate::ir::IrMut;
#[cfg(feature = "internal_error_chain")]
use crate::ir::{EdgeKind, NodeKind, NodeWeight};
#[cfg(feature = "internal_error_chain")]
use crate::objects::FileSpan;
use crate::objects::SourceSpan;
use std::collections::HashMap;
use tracing::instrument;

#[instrument(level = "debug", skip(view), err(level = "warn"))]
pub(super) fn enrich(view: SourceEnrichView<'_>) -> CordialResult<()> {
    let (ir, source, session) = view.into_parts();

    let crate_root = member_crate_root(source, session);
    #[cfg(feature = "internal_error_chain")]
    let error_config = crate::config::load_session_config(session);
    #[cfg(feature = "internal_error_chain")]
    let kind_fanout_max_call_sites = error_config
        .internal_error_chain()
        .kind_fanout_max_call_sites();
    #[cfg(not(feature = "internal_error_chain"))]
    let kind_fanout_max_call_sites = 0;
    #[cfg(feature = "internal_error_chain")]
    let field_strategy = error_config.internal_error_chain().field_strategy();
    #[cfg(not(feature = "internal_error_chain"))]
    let field_strategy = crate::config::ErrorFieldStrategy::default();
    let report = scan_crate_error_ir_with_source(
        &crate_root,
        ir.crate_name(),
        source,
        kind_fanout_max_call_sites,
        field_strategy,
    )?;
    materialize_error_ir(ir, &crate_root, &report)?;
    #[cfg(feature = "internal_error_chain")]
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
    let mut site_nodes: HashMap<(String, u32), Vec<crate::ir::NodeId>> = HashMap::new();
    for node in ir.nodes_matching(
        &crate::ir::QueryBuilder::new()
            .has_attr("error_site_kind")
            .build(),
    ) {
        let span = node.weight().span().as_ref().ok_or_else(|| {
            crate::CordialError::workspace_source_facts_missing("located error-site node")
        })?;
        let file = span.file().strip_prefix(crate_root).unwrap_or(span.file());
        site_nodes
            .entry(site_key(file, span.line()))
            .or_default()
            .push(node.id());
    }

    for record in report.chain() {
        let key = site_key(record.file(), record.line());
        if let Some(nodes) = site_nodes.get(&key) {
            for &node in nodes {
                chain::apply_chain_attrs(ir, node, record)?;
            }
        } else {
            chain::insert_error_chain_node(ir, crate_root, record)?;
        }
    }

    #[cfg(feature = "internal_error_chain")]
    {
        for node in report.type_graph().nodes() {
            let parent = resolve_parent(ir, node.type_path())?;
            let file = crate_root.join(node.file());
            let span = FileSpan::new(file.clone(), node.line(), 1);
            let ir_node = ir.insert_node(
                NodeWeight::new(NodeKind::Expr)
                    .with_span(span.clone())
                    .with_name(node.snippet().clone()),
            )?;
            ir.set_attr(
                ir_node,
                "internal_error_record_kind",
                serde_json::Value::String("type_graph".to_string()),
            )?;
            ir.set_attr(
                ir_node,
                "type_path",
                serde_json::Value::String(node.type_path().clone()),
            )?;
            ir.set_attr(
                ir_node,
                "node_class",
                serde_json::Value::String(node.node_class().to_string()),
            )?;
            ir.set_attr(
                ir_node,
                "probe_id",
                serde_json::Value::String(node.probe_id().to_string()),
            )?;
            ir.set_attr(
                ir_node,
                "source_target",
                serde_json::Value::String(node.source_target().clone().unwrap_or_default()),
            )?;
            ir.set_attr(
                ir_node,
                "reaches_foreign",
                serde_json::Value::Bool(node.reaches_foreign()),
            )?;
            ir.set_attr(
                ir_node,
                "chain_depth",
                serde_json::Value::Number(node.chain_depth().into()),
            )?;
            ir.set_attr(
                ir_node,
                "snippet",
                serde_json::Value::String(node.snippet().clone()),
            )?;
            ir.set_attr(
                ir_node,
                "file",
                serde_json::Value::String(file.display().to_string()),
            )?;
            ir.set_attr(
                ir_node,
                "line",
                serde_json::Value::Number(node.line().into()),
            )?;
            ir.insert_edge(parent, ir_node, EdgeKind::Contains)?;
        }

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

#[instrument(level = "debug", skip(file))]
fn site_key(file: &std::path::Path, line: u32) -> (String, u32) {
    (file.display().to_string(), line)
}

/// `error_chain` node materialization, gated as a whole unit — see
/// `docs/planning/cfg-scatter-etiquette.md` for the pattern.
mod chain {
    use crate::enricher::resolve_parent;
    use crate::error::CordialResult;
    use crate::etiquettes::error_chain::ErrorChainRecord;
    use crate::ir::{EdgeKind, IrMut, NodeKind, NodeWeight};
    use crate::objects::FileSpan;
    use tracing::instrument;

    #[instrument(level = "debug", skip(ir, node, record), err(level = "warn"))]
    pub(super) fn apply_chain_attrs(
        ir: &mut dyn IrMut,
        node: crate::ir::NodeId,
        record: &ErrorChainRecord,
    ) -> CordialResult<()> {
        ir.set_attr(
            node,
            "error_chain_rule_id",
            serde_json::Value::String(record.rule_id().as_str().to_string()),
        )?;
        if !ir.node(node).is_some_and(|n| n.attr("context").is_some()) {
            ir.set_attr(
                node,
                "context",
                serde_json::Value::String(record.context().clone()),
            )?;
        }
        ir.set_attr(
            node,
            "snippet",
            serde_json::Value::String(record.snippet().clone()),
        )?;
        ir.set_attr(
            node,
            "foreign_error_type",
            serde_json::Value::String(record.foreign_error_type().clone().unwrap_or_default()),
        )?;
        Ok(())
    }

    #[instrument(level = "debug", skip(ir, record), err(level = "warn"))]
    pub(super) fn insert_error_chain_node(
        ir: &mut dyn IrMut,
        crate_root: &std::path::Path,
        record: &ErrorChainRecord,
    ) -> CordialResult<()> {
        let parent = resolve_parent(ir, record.context())?;
        let file = crate_root.join(record.file());
        let span = FileSpan::new(file.clone(), record.line(), 1);
        let node = ir.insert_node(
            NodeWeight::new(NodeKind::Expr)
                .with_span(span.clone())
                .with_name(record.snippet().clone()),
        )?;
        apply_chain_attrs(ir, node, record)?;
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
        Ok(())
    }
}
