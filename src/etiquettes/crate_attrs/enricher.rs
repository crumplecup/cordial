use crate::enricher::{member_crate_root, resolve_parent};
use crate::error::CordialResult;
use crate::hooks::{SourceEnrichView, SourceEnricher};
use crate::ir::{EdgeKind, IrMut, IrView, NodeKind, NodeWeight, QueryBuilder};
use crate::loader::SourceLoadView;
use crate::objects::FileSpan;
use syn::spanned::Spanned;

use super::scan::{library_root_rs, scan_shared_crate_attrs};
use super::types::CrateAttrsSiteRecord;

const RECORD_ATTR: &str = "crate_attrs_record";
const INSERT_AT_ATTR: &str = "crate_attrs_insert_at";

use tracing::instrument;

/// Materializes crate-attribute expression nodes in the IR graph.
#[derive(Debug, Default, Clone, Copy)]
pub struct CrateAttrsInventoryEnricher;

impl CrateAttrsInventoryEnricher {
    pub const ID: &'static str = "crate-attrs-inventory";
}

impl SourceEnricher for CrateAttrsInventoryEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich_source(&self, view: SourceEnrichView<'_>) -> CordialResult<()> {
        let (ir, source, session) = view.into_parts();

        let crate_root = member_crate_root(source, session);
        let policy = crate::config::load_session_config(session)
            .crate_attrs()
            .clone();
        enrich_crate_attrs_evidence(ir, source, &crate_root, &policy)
    }
}

/// Collect the crate-attribute decision and insertion position from one shared tree.
#[instrument(level = "debug", skip(ir, source, policy), err(level = "warn"))]
pub(super) fn enrich_crate_attrs_evidence(
    ir: &mut dyn IrMut,
    source: &SourceLoadView,
    crate_root: &std::path::Path,
    policy: &crate::config::CrateAttrsThresholds,
) -> CordialResult<()> {
    let records = scan_shared_crate_attrs(source, crate_root, ir.crate_name(), policy)?;
    let insert_at = if let Some(lib) = library_root_rs(crate_root)
        && lib.is_file()
    {
        let file = source.parsed_file(&lib, CrateAttrsInventoryEnricher::ID)?;
        Some(
            file.syntax()
                .items
                .first()
                .map(|item| item.span().start().line.saturating_sub(1))
                .unwrap_or_else(|| file.source().lines().count()),
        )
    } else {
        None
    };

    for record in records {
        let parent = resolve_parent(ir, "<crate>")?;
        let file = crate_root.join(record.file());
        let span = FileSpan::new(file.clone(), record.line(), 1);
        let node = ir.insert_node(
            NodeWeight::new(NodeKind::Expr)
                .with_span(span)
                .with_name(record.snippet().clone()),
        )?;
        ir.set_attr(
            node,
            "crate_attrs_rule_id",
            serde_json::Value::String(record.rule_id().as_str().to_string()),
        )?;
        ir.set_attr(node, RECORD_ATTR, serde_json::to_value(&record)?)?;
        if let Some(insert_at) = insert_at {
            ir.set_attr(node, INSERT_AT_ATTR, serde_json::json!(insert_at))?;
        }
        ir.set_attr(
            node,
            "context",
            serde_json::Value::String(record.context().clone()),
        )?;
        ir.set_attr(
            node,
            "snippet",
            serde_json::Value::String(record.snippet().clone()),
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
    }

    Ok(())
}

/// Read apply evidence only from the enriched graph.
#[instrument(level = "debug", skip(ir), err(level = "warn"))]
pub(super) fn crate_attrs_apply_evidence_from_ir(
    ir: &dyn IrView,
) -> CordialResult<Option<(Vec<CrateAttrsSiteRecord>, usize)>> {
    let query = QueryBuilder::new().has_attr(RECORD_ATTR).build();
    let evidence = ir
        .nodes_matching(&query)
        .into_iter()
        .map(|node| {
            let record =
                serde_json::from_value(node.attr(RECORD_ATTR).cloned().unwrap_or_default())?;
            let insert_at =
                serde_json::from_value(node.attr(INSERT_AT_ATTR).cloned().unwrap_or_default())?;
            Ok((record, insert_at))
        })
        .collect::<CordialResult<Vec<(CrateAttrsSiteRecord, usize)>>>()?;
    let Some((_, insert_at)) = evidence.first() else {
        return Ok(None);
    };
    let insert_at = *insert_at;
    let records = evidence.into_iter().map(|(record, _)| record).collect();
    Ok(Some((records, insert_at)))
}
