//! Enum payload relationships, preserving the current error policy grammar.

use super::Fact;
use crate::etiquettes::internal_error_chain::InternalErrorTypeProbeId;
use crate::ir::error_chains_source::types::{is_string, type_label};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasName},
};

#[tracing::instrument(level = "trace", skip(item))]
pub(super) fn collect(item: &ast::Enum) -> Vec<Fact> {
    let Some(name) = item.name() else {
        return Vec::new();
    };
    let name = name.text().to_string();
    item.variant_list()
        .into_iter()
        .flat_map(|list| list.variants())
        .filter_map(|variant| {
            let ident = variant.name()?.text().to_string();
            let path = format!("{name}::{ident}");
            let (probe, target, snippet) = match variant.field_list()? {
                ast::FieldList::RecordFieldList(fields) => {
                    let mut has_source = false;
                    let mut has_detail = false;
                    let mut target = None;
                    for field in fields.fields() {
                        let Some(ident) = field.name() else { continue };
                        let ty = field.ty()?;
                        if matches!(ident.text(), "source" | "err") {
                            has_source = true;
                            if !is_string(&ty) {
                                target = Some(type_label(&ty));
                            }
                        }
                        if matches!(ident.text(), "detail" | "path" | "message") && is_string(&ty) {
                            has_detail = true;
                        }
                    }
                    if has_detail && !has_source {
                        (
                            InternalErrorTypeProbeId::InternalLeaf001,
                            None,
                            format!("enum {name} {{ {path} {{ … }} }}"),
                        )
                    } else {
                        (
                            InternalErrorTypeProbeId::InternalLink001,
                            Some(target?),
                            format!("enum {name} {{ {ident} {{ source: … }} }}"),
                        )
                    }
                }
                ast::FieldList::TupleFieldList(fields) => {
                    let mut fields = fields.fields();
                    let ty = fields.next()?.ty()?;
                    if fields.next().is_some() {
                        return None;
                    }
                    if is_string(&ty) {
                        (
                            InternalErrorTypeProbeId::InternalLeaf001,
                            None,
                            format!("enum {name} {{ {path}(String) }}"),
                        )
                    } else {
                        (
                            InternalErrorTypeProbeId::InternalLink001,
                            Some(type_label(&ty)),
                            format!("enum {name} {{ {ident}({}) }}", type_label(&ty)),
                        )
                    }
                }
            };
            Some(Fact {
                path,
                probe,
                target,
                node: variant.syntax().clone(),
                snippet,
            })
        })
        .collect()
}
