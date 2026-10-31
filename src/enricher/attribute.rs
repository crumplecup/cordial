//! Materialize attribute nodes from the loader's shared IR inventory.

use crate::error::CordialResult;
use crate::hooks::{SourceEnrichView, SourceEnricher};
use crate::ir::{EdgeKind, IrMut, ItemKind, NodeKind, NodeWeight, QueryBuilder};
use crate::objects::{FileSpan, SourceSpan};
use tracing::instrument;

pub use crate::ir::{is_cfg_test, is_gated_instrument_attr, is_instrument_attr};

/// Materializes `Attribute` nodes linked to items via [`EdgeKind::HasAttr`].
#[derive(Debug, Default, Clone, Copy)]
pub struct AttributeEnricher;

impl AttributeEnricher {
    /// Stable identifier for `AttributeEnricher`.
    pub const ID: &'static str = "attribute";
}

impl SourceEnricher for AttributeEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        6
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich_source(&self, view: SourceEnrichView<'_>) -> CordialResult<()> {
        let (ir, _, session) = view.into_parts();
        let query = QueryBuilder::new()
            .has_attr(crate::ir::ATTR_SOURCE_FILE_ATTRIBUTES)
            .build();
        let modules: Vec<_> = ir
            .nodes_matching(&query)
            .into_iter()
            .map(|node| node.id())
            .collect();
        for module in modules {
            let facts = ir.source_file_attributes(module)?.ok_or_else(|| {
                crate::CordialError::workspace_source_facts_missing("source attributes")
            })?;
            let node = ir.node(module).ok_or_else(|| {
                crate::CordialError::workspace_source_facts_missing("source module")
            })?;
            let file = node
                .weight()
                .span()
                .as_ref()
                .ok_or_else(|| {
                    crate::CordialError::workspace_source_facts_missing("source module span")
                })?
                .file()
                .to_path_buf();
            let prefix = node
                .attr("qualified_path")
                .and_then(|value| value.as_str())
                .ok_or_else(|| {
                    crate::CordialError::workspace_source_facts_missing("source module path")
                })?
                .to_string();
            let relative = file
                .strip_prefix(session.project_root())
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            for item in facts.items() {
                let path = if prefix.is_empty() {
                    item.path().clone()
                } else {
                    format!("{prefix}::{}", item.path())
                };
                attach(ir, &file, &relative, &path, item)?;
            }
        }
        Ok(())
    }
}

#[instrument(level = "debug", skip(ir, file, path, item), err(level = "warn"))]
fn attach(
    ir: &mut dyn IrMut,
    file: &std::path::Path,
    relative: &str,
    path: &str,
    item: &crate::ir::SourceAttributedItem,
) -> CordialResult<()> {
    let Some(owner) = ir.source_node_at(path, file, item.line())? else {
        return Ok(());
    };
    for attr in item.attributes() {
        let node = ir.insert_node(
            NodeWeight::new(NodeKind::Attribute)
                .with_name(attr.path().clone())
                .with_span(FileSpan::new(file, item.line(), 1)),
        )?;
        ir.set_attr(node, "attr_path", attr.path().clone().into())?;
        ir.set_attr(node, "meta", attr.meta().clone().into())?;
        ir.set_attr(node, "file", relative.into())?;
        ir.set_attr(
            node,
            crate::ir::ATTR_SOURCE_ATTRIBUTE,
            serde_json::to_value(attr)?,
        )?;
        ir.insert_edge(owner, node, EdgeKind::HasAttr)?;
    }
    if ir.node(owner).is_some_and(|node| {
        matches!(node.kind(), NodeKind::Item(ItemKind::Fn)) && node.attr("function_kind").is_some()
    }) {
        ir.set_attr(
            owner,
            "instrumented",
            item.attributes()
                .iter()
                .any(|attr| attr.instrument())
                .into(),
        )?;
    }
    Ok(())
}
