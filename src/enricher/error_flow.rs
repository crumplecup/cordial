//! Partitions error-site nodes and links them to inferred origins via `ErrorFlow` edges.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::enricher::member_crate_root;
use crate::error::CordialResult;
use crate::etiquettes::error_sites::{
    ErrorOriginClass, ErrorSiteKind, ErrorSiteScanRow, ForeignErrorRecordKind,
    infer_foreign_error_type, partition_error_site_row,
};
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{BasicQuery, EdgeKind, IrMut, NodeKind, NodeWeight};
use crate::loader::SourceLoadView;

use tracing::instrument;
/// Partitions error-site expression nodes and materializes `ErrorFlow` origin links.
#[derive(Debug, Default, Clone, Copy)]
pub struct ErrorFlowEnricher;

impl ErrorFlowEnricher {
    /// Stable identifier for `ErrorFlowEnricher`.
    pub const ID: &'static str = "error-flow";
    /// IR attribute key (`error_flow_origin`).
    pub const ATTR_ERROR_FLOW_ORIGIN: &'static str = "error_flow_origin";
}

impl IrEnricher for ErrorFlowEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        51
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let ir = view.ir;
        let load = view.load;
        let session = view.session;

        let crate_name = ir.crate_name().to_string();
        let crate_root = ir.root()?;
        let is_proc_macro_crate = load
            .as_any()
            .downcast_ref::<SourceLoadView>()
            .is_some_and(|source| crate_is_proc_macro(&member_crate_root(source, session)));
        let mut origin_nodes: BTreeMap<String, crate::ir::NodeId> = BTreeMap::new();

        let site_ids: Vec<_> = ir
            .nodes_matching(&BasicQuery::all_nodes())
            .into_iter()
            .filter(|node| matches!(node.kind(), NodeKind::Expr))
            .filter(|node| node.attr("error_site_kind").is_some())
            .map(|node| node.id)
            .collect();

        for site_id in site_ids {
            enrich_error_site(
                ir,
                site_id,
                &crate_name,
                crate_root,
                is_proc_macro_crate,
                &mut origin_nodes,
            )?;
        }

        Ok(())
    }
}

/// Whether `crate_root`'s manifest declares `[lib] proc-macro = true`.
///
/// `syn::Error`/`syn::Result` are the idiomatic return-error currency for
/// proc-macro expansion helpers industry-wide -- not a foreign leak that
/// needs wrapping, since the only consumer is the compiler (the macro's
/// public entry point returns a bare `TokenStream`, never a `Result`).
#[instrument(level = "debug", skip(crate_root))]
fn crate_is_proc_macro(crate_root: &Path) -> bool {
    let Ok(manifest) = std::fs::read_to_string(crate_root.join("Cargo.toml")) else {
        return false;
    };
    let mut in_lib_section = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if let Some(section) = trimmed
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            in_lib_section = section.trim() == "lib";
            continue;
        }
        if in_lib_section
            && let Some((key, value)) = trimmed.split_once('=')
            && key.trim() == "proc-macro"
            && value.trim().starts_with("true")
        {
            return true;
        }
    }
    false
}

#[instrument(
    level = "debug",
    skip(ir, site_id, crate_root, origin_nodes),
    err(level = "warn")
)]
fn enrich_error_site(
    ir: &mut dyn IrMut,
    site_id: crate::ir::NodeId,
    crate_name: &str,
    crate_root: crate::ir::NodeId,
    is_proc_macro_crate: bool,
    origin_nodes: &mut BTreeMap<String, crate::ir::NodeId>,
) -> CordialResult<()> {
    let Some(scan_row) = scan_row_from_node(ir, site_id, crate_name)? else {
        return Ok(());
    };
    let partitioned = partition_error_site_row(&scan_row, crate_name)?;

    ir.set_attr(
        site_id,
        "origin_class",
        serde_json::Value::String(partitioned.origin_class().to_string()),
    )?;
    ir.set_attr(
        site_id,
        "origin_detail",
        serde_json::Value::String(partitioned.origin_detail().clone()),
    )?;
    ir.set_attr(
        site_id,
        "rationale",
        serde_json::Value::String(partitioned.rationale().clone()),
    )?;

    let origin_key = origin_key_for(&partitioned);
    let origin_id = origin_node(ir, origin_nodes, crate_root, &origin_key)?;
    ir.insert_edge(site_id, origin_id, EdgeKind::ErrorFlow)?;

    apply_foreign_error_attrs(ir, site_id, &partitioned, is_proc_macro_crate)?;
    Ok(())
}

#[instrument(level = "debug", skip(ir, site_id))]
fn scan_row_from_node(
    ir: &dyn IrMut,
    site_id: crate::ir::NodeId,
    crate_name: &str,
) -> CordialResult<Option<ErrorSiteScanRow>> {
    let Some(node) = ir.node(site_id) else {
        return Ok(None);
    };
    let Some(kind) = node
        .attr("error_site_kind")
        .and_then(|value| value.as_str())
        .and_then(ErrorSiteKind::from_attr)
    else {
        return Ok(None);
    };
    let context = node
        .attr("context")
        .and_then(|value| value.as_str())
        .unwrap_or("<crate>")
        .to_string();
    let source_snippet = node
        .attr("source_snippet")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let site_snippet = node
        .attr("site_snippet")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let line = node
        .attr("line")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as u32;
    let file = node
        .attr("file")
        .and_then(|value| value.as_str())
        .map(PathBuf::from)
        .unwrap_or_default();

    Ok(Some(
        ErrorSiteScanRow::builder()
            .crate_name(crate_name.to_string())
            .kind(kind)
            .context(context)
            .file(file)
            .line(line)
            .source_snippet(source_snippet)
            .site_snippet(site_snippet)
            .build()?,
    ))
}

#[instrument(level = "debug", skip(partitioned))]
fn origin_key_for(partitioned: &crate::etiquettes::error_sites::PartitionedErrorSiteRow) -> String {
    if partitioned.origin_class() == ErrorOriginClass::Internal {
        format!("internal:{}", partitioned.origin_detail())
    } else {
        format!(
            "{}:{}",
            partitioned.origin_class(),
            partitioned.origin_detail()
        )
    }
}

#[instrument(
    level = "debug",
    skip(ir, origin_nodes, crate_root),
    err(level = "warn")
)]
fn origin_node(
    ir: &mut dyn IrMut,
    origin_nodes: &mut BTreeMap<String, crate::ir::NodeId>,
    crate_root: crate::ir::NodeId,
    origin_key: &str,
) -> CordialResult<crate::ir::NodeId> {
    if let Some(node_id) = origin_nodes.get(origin_key) {
        return Ok(*node_id);
    }

    let node_id =
        ir.insert_node(NodeWeight::new(NodeKind::Type).with_name(origin_key.to_string()))?;
    ir.set_attr(
        node_id,
        "qualified_path",
        serde_json::Value::String(origin_key.to_string()),
    )?;
    ir.set_attr(
        node_id,
        ErrorFlowEnricher::ATTR_ERROR_FLOW_ORIGIN,
        serde_json::Value::Bool(true),
    )?;
    ir.insert_edge(crate_root, node_id, EdgeKind::Contains)?;
    origin_nodes.insert(origin_key.to_string(), node_id);
    Ok(node_id)
}

#[instrument(level = "debug", skip(ir, site_id, partitioned), err(level = "warn"))]
fn apply_foreign_error_attrs(
    ir: &mut dyn IrMut,
    site_id: crate::ir::NodeId,
    partitioned: &crate::etiquettes::error_sites::PartitionedErrorSiteRow,
    is_proc_macro_crate: bool,
) -> CordialResult<()> {
    if partitioned.origin_class() == ErrorOriginClass::Internal {
        return Ok(());
    }

    if let Some((foreign_error_type, rule_id, confidence)) =
        infer_foreign_error_type(partitioned.source_snippet())
    {
        if is_proc_macro_crate && rule_id == "FOREIGN-ERROR-TYPE-SYN-PARSE-001" {
            return Ok(());
        }

        ir.set_attr(
            site_id,
            "foreign_error_record_kind",
            serde_json::Value::String(ForeignErrorRecordKind::Typed.as_attr().to_string()),
        )?;
        ir.set_attr(
            site_id,
            "foreign_error_type",
            serde_json::Value::String(foreign_error_type),
        )?;
        ir.set_attr(
            site_id,
            "inference_rule_id",
            serde_json::Value::String(rule_id),
        )?;
        ir.set_attr(
            site_id,
            "confidence",
            serde_json::Value::String(confidence.to_string()),
        )?;
        let chain_preserved = ir.node(site_id).is_some_and(|node| {
            node.attr("error_chain_rule_id")
                .and_then(|value| value.as_str())
                .is_some_and(chain_probe_preserves)
        });
        ir.set_attr(
            site_id,
            "chain_break",
            serde_json::Value::Bool(partitioned.kind().map_err_is_chain_break(chain_preserved)),
        )?;
        return Ok(());
    }

    if matches!(
        partitioned.origin_class(),
        ErrorOriginClass::Other | ErrorOriginClass::Edge
    ) {
        ir.set_attr(
            site_id,
            "foreign_error_record_kind",
            serde_json::Value::String(ForeignErrorRecordKind::Candidate.as_attr().to_string()),
        )?;
    }

    Ok(())
}

#[instrument(level = "debug")]
fn chain_probe_preserves(rule_id: &str) -> bool {
    rule_id == "ERROR-CHAIN-PRESERVED-MAP-ERR-001"
        || rule_id == "ERROR-CHAIN-PRESERVED-QUESTION-MARK-001"
}
