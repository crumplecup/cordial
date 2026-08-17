//! Materializes unified error IR scan results into the graph.

use std::collections::HashMap;

use crate::enricher::{member_crate_root, resolve_parent};
use crate::error::CordialResult;
use crate::etiquettes::error_sites::ErrorSiteRecord;
use crate::hooks::IrEnricher;
use crate::ir::{EdgeKind, IrMut, NodeKind, NodeWeight};
use crate::loader::{LoadView, SourceLoadView};
use crate::objects::FileSpan;
use crate::session::SessionView;

use super::scan::scan_crate_error_ir;

/// Single enricher pass: unified scan → IR nodes (sites and chain merged by location).
#[derive(Debug, Default, Clone, Copy)]
pub struct ErrorIrScanEnricher;

impl ErrorIrScanEnricher {
    pub const ID: &'static str = "error-ir-scan";
}

impl IrEnricher for ErrorIrScanEnricher {
    fn id(&self) -> &str {
        Self::ID
    }

    fn priority(&self) -> u8 {
        50
    }

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
        let report = scan_crate_error_ir(&crate_root, ir.crate_name())?;
        materialize_error_ir(ir, &crate_root, &report)?;
        Ok(())
    }
}

fn materialize_error_ir(
    ir: &mut dyn IrMut,
    crate_root: &std::path::Path,
    report: &super::scan::ErrorIrScanReport,
) -> CordialResult<()> {
    let mut site_nodes: HashMap<(String, u32), Vec<crate::ir::NodeId>> = HashMap::new();

    for record in &report.sites {
        let node = insert_error_site_node(ir, crate_root, record)?;
        let key = site_key(&record.file, record.line);
        site_nodes.entry(key).or_default().push(node);
    }

    #[cfg(feature = "error_chain")]
    for record in &report.chain {
        let key = site_key(&record.file, record.line);
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
        for node in &report.type_graph.nodes {
            let parent = resolve_parent(ir, &node.type_path)?;
            let file = crate_root.join(&node.file);
            let span = FileSpan::new(file.clone(), node.line, 1);
            let ir_node = ir.insert_node(
                NodeWeight::new(NodeKind::Expr)
                    .with_span(span.clone())
                    .with_name(node.snippet.clone()),
            )?;
            ir.set_attr(
                ir_node,
                "internal_error_record_kind",
                serde_json::Value::String("type_graph".to_string()),
            )?;
            ir.set_attr(
                ir_node,
                "type_path",
                serde_json::Value::String(node.type_path.clone()),
            )?;
            ir.set_attr(
                ir_node,
                "node_class",
                serde_json::Value::String(node.node_class.to_string()),
            )?;
            ir.set_attr(
                ir_node,
                "probe_id",
                serde_json::Value::String(node.probe_id.to_string()),
            )?;
            ir.set_attr(
                ir_node,
                "source_target",
                serde_json::Value::String(node.source_target.clone().unwrap_or_default()),
            )?;
            ir.set_attr(
                ir_node,
                "reaches_foreign",
                serde_json::Value::Bool(node.reaches_foreign),
            )?;
            ir.set_attr(
                ir_node,
                "chain_depth",
                serde_json::Value::Number(node.chain_depth.into()),
            )?;
            ir.set_attr(
                ir_node,
                "snippet",
                serde_json::Value::String(node.snippet.clone()),
            )?;
            ir.set_attr(
                ir_node,
                "file",
                serde_json::Value::String(file.display().to_string()),
            )?;
            ir.set_attr(ir_node, "line", serde_json::Value::Number(node.line.into()))?;
            ir.insert_edge(parent, ir_node, EdgeKind::Contains)?;
        }

        for finding in &report.compliance {
            let parent = resolve_parent(ir, &finding.context)?;
            let file = crate_root.join(&finding.file);
            let span = FileSpan::new(file.clone(), finding.line, 1);
            let ir_node = ir.insert_node(
                NodeWeight::new(NodeKind::Expr)
                    .with_span(span.clone())
                    .with_name(finding.snippet.clone()),
            )?;
            ir.set_attr(
                ir_node,
                "internal_error_record_kind",
                serde_json::Value::String("compliance".to_string()),
            )?;
            ir.set_attr(
                ir_node,
                "rule_id",
                serde_json::Value::String(finding.rule_id.to_string()),
            )?;
            ir.set_attr(
                ir_node,
                "context",
                serde_json::Value::String(finding.context.clone()),
            )?;
            ir.set_attr(
                ir_node,
                "foreign_error_type",
                serde_json::Value::String(finding.foreign_error_type.clone().unwrap_or_default()),
            )?;
            ir.set_attr(
                ir_node,
                "internal_constructor",
                serde_json::Value::String(finding.internal_constructor.clone().unwrap_or_default()),
            )?;
            ir.set_attr(
                ir_node,
                "snippet",
                serde_json::Value::String(finding.snippet.clone()),
            )?;
            ir.set_attr(
                ir_node,
                "file",
                serde_json::Value::String(file.display().to_string()),
            )?;
            ir.set_attr(
                ir_node,
                "line",
                serde_json::Value::Number(finding.line.into()),
            )?;
            ir.insert_edge(parent, ir_node, EdgeKind::Contains)?;
        }
    }

    Ok(())
}

fn site_key(file: &std::path::Path, line: u32) -> (String, u32) {
    (file.display().to_string(), line)
}

fn insert_error_site_node(
    ir: &mut dyn IrMut,
    crate_root: &std::path::Path,
    record: &ErrorSiteRecord,
) -> CordialResult<crate::ir::NodeId> {
    let parent = resolve_parent(ir, &record.context)?;
    let file = crate_root.join(&record.file);
    let span = FileSpan::new(file.clone(), record.line, 1);
    let node = ir.insert_node(
        NodeWeight::new(NodeKind::Expr)
            .with_span(span.clone())
            .with_name(record.site_snippet.clone()),
    )?;
    ir.set_attr(
        node,
        "error_site_kind",
        serde_json::Value::String(record.kind.as_attr().to_string()),
    )?;
    ir.set_attr(
        node,
        "context",
        serde_json::Value::String(record.context.clone()),
    )?;
    ir.set_attr(
        node,
        "source_snippet",
        serde_json::Value::String(record.source_snippet.clone()),
    )?;
    ir.set_attr(
        node,
        "site_snippet",
        serde_json::Value::String(record.site_snippet.clone()),
    )?;
    ir.set_attr(
        node,
        "file",
        serde_json::Value::String(file.display().to_string()),
    )?;
    ir.set_attr(node, "line", serde_json::Value::Number(record.line.into()))?;
    ir.insert_edge(parent, node, EdgeKind::Contains)?;
    Ok(node)
}

/// `error_chain` node materialization, gated as a whole unit — see
/// `docs/planning/cfg-scatter-etiquette.md` for the pattern.
#[cfg(feature = "error_chain")]
mod chain {
    use super::*;
    use crate::etiquettes::error_chain::ErrorChainRecord;

    pub(super) fn apply_chain_attrs(
        ir: &mut dyn IrMut,
        node: crate::ir::NodeId,
        record: &ErrorChainRecord,
    ) -> CordialResult<()> {
        ir.set_attr(
            node,
            "error_chain_rule_id",
            serde_json::Value::String(record.rule_id.as_str().to_string()),
        )?;
        if !ir.node(node).is_some_and(|n| n.attr("context").is_some()) {
            ir.set_attr(
                node,
                "context",
                serde_json::Value::String(record.context.clone()),
            )?;
        }
        ir.set_attr(
            node,
            "snippet",
            serde_json::Value::String(record.snippet.clone()),
        )?;
        ir.set_attr(
            node,
            "foreign_error_type",
            serde_json::Value::String(record.foreign_error_type.clone().unwrap_or_default()),
        )?;
        Ok(())
    }

    pub(super) fn insert_error_chain_node(
        ir: &mut dyn IrMut,
        crate_root: &std::path::Path,
        record: &ErrorChainRecord,
    ) -> CordialResult<()> {
        let parent = resolve_parent(ir, &record.context)?;
        let file = crate_root.join(&record.file);
        let span = FileSpan::new(file.clone(), record.line, 1);
        let node = ir.insert_node(
            NodeWeight::new(NodeKind::Expr)
                .with_span(span.clone())
                .with_name(record.snippet.clone()),
        )?;
        apply_chain_attrs(ir, node, record)?;
        ir.set_attr(
            node,
            "file",
            serde_json::Value::String(file.display().to_string()),
        )?;
        ir.set_attr(node, "line", serde_json::Value::Number(record.line.into()))?;
        ir.insert_edge(parent, node, EdgeKind::Contains)?;
        Ok(())
    }
}
