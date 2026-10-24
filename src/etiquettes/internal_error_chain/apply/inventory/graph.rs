//! Materialization and typed reads of conversion evidence in the shared IR.

use syn::Item;
use syn::spanned::Spanned;
use tracing::instrument;

use crate::error::CordialResult;
use crate::ir::{EdgeKind, IrMut, IrView, NodeKind, NodeWeight, QueryBuilder};
use crate::loader::SharedParsedFile;
use crate::objects::FileSpan;

use super::super::{is_error_string_field, known_error_impls, names::audit_names};
use super::ErrorConversionUnit;
use crate::etiquettes::internal_error_chain::type_graph::item_derives_error;

const CONVERSION_UNIT_ATTR: &str = "error_conversion_unit";

/// Collect conversion evidence before apply and materialize it in the shared IR.
#[instrument(level = "debug", skip(ir, files), err(level = "warn"))]
pub(crate) fn enrich_conversion_units(
    ir: &mut dyn IrMut,
    files: &[SharedParsedFile],
) -> CordialResult<()> {
    let root = ir.root()?;
    for file in files {
        let syntax = file.syntax();
        let error_impls = known_error_impls(syntax);
        let name_audit = audit_names(syntax);
        for item in &syntax.items {
            let Item::Struct(item) = item else { continue };
            let type_name = item.ident.to_string();
            if !error_impls.contains(&type_name) && !item_derives_error(&item.attrs) {
                continue;
            }
            let syn::Fields::Named(fields) = &item.fields else {
                continue;
            };
            for field in &fields.named {
                let Some(field_name) = field.ident.as_ref().map(ToString::to_string) else {
                    continue;
                };
                if matches!(field_name.as_str(), "source" | "kind") {
                    continue;
                }
                if !is_error_string_field(&field.ty) {
                    continue;
                }
                let unit = ErrorConversionUnit::collect(
                    files,
                    file.path(),
                    &type_name,
                    &field_name,
                    name_audit.blocks(&type_name),
                );
                let node = ir.insert_node(
                    NodeWeight::new(NodeKind::Field)
                        .with_name(field_name)
                        .with_span(FileSpan::new(
                            file.path(),
                            field.span().start().line as u32,
                            1,
                        )),
                )?;
                ir.set_attr(node, CONVERSION_UNIT_ATTR, serde_json::to_value(unit)?)?;
                ir.insert_edge(root, node, EdgeKind::Contains)?;
            }
        }
    }
    Ok(())
}

/// Read conversion evidence from the graph, rejecting malformed facts.
#[instrument(level = "debug", skip(ir), err(level = "warn"))]
pub(crate) fn conversion_units_from_ir(ir: &dyn IrView) -> CordialResult<Vec<ErrorConversionUnit>> {
    let query = QueryBuilder::new().has_attr(CONVERSION_UNIT_ATTR).build();
    ir.nodes_matching(&query)
        .into_iter()
        .map(|node| {
            let value = node
                .attr(CONVERSION_UNIT_ATTR)
                .ok_or_else(|| crate::error::CordialError::unreachable(CONVERSION_UNIT_ATTR))?;
            Ok(serde_json::from_value(value.clone())?)
        })
        .collect()
}
