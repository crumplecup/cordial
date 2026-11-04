//! Field visibility and wrapper shapes used by derive policy.
use crate::{CordialResult, ir::DeriveFieldFacts};
use ra_ap_syntax::ast::{self, HasGenericArgs, HasVisibility};
use std::collections::BTreeMap;

type Fields = (BTreeMap<String, DeriveFieldFacts>, Vec<String>);

#[tracing::instrument(level = "trace", skip(item), err(level = "warn"))]
pub(super) fn collect(item: &ast::Struct) -> CordialResult<Fields> {
    let mut fields = BTreeMap::new();
    let mut exposed = Vec::new();
    match item.field_list() {
        Some(ast::FieldList::RecordFieldList(list)) => {
            for field in list.fields() {
                let name = super::name(&field)?;
                let ty = field
                    .ty()
                    .ok_or_else(|| crate::CordialError::not_found("derive field type".into()))?;
                let public = field.visibility().is_some();
                fields.insert(
                    name.clone(),
                    DeriveFieldFacts::new(public, is_option(&ty), boxed_inner(ty)),
                );
                if public {
                    exposed.push(name);
                }
            }
        }
        Some(ast::FieldList::TupleFieldList(list)) => {
            for (index, field) in list.fields().enumerate() {
                if field.visibility().is_some() {
                    exposed.push(format!("_{index}"));
                }
            }
        }
        None => {}
    }
    Ok((fields, exposed))
}

#[tracing::instrument(level = "trace", skip(ty))]
fn is_option(ty: &ast::Type) -> bool {
    matches!(ty, ast::Type::PathType(ty) if ty.path().and_then(|path| path.segment()).and_then(|segment| segment.name_ref()).is_some_and(|name| name.text() == "Option"))
}

#[tracing::instrument(level = "trace", skip(ty))]
fn boxed_inner(ty: ast::Type) -> Option<String> {
    match ty {
        ast::Type::ParenType(ty) => boxed_inner(ty.ty()?),
        ast::Type::PathType(ty) => {
            let segment = ty.path()?.segment()?;
            if segment.name_ref()?.text() != "Box" {
                return None;
            }
            segment
                .generic_arg_list()?
                .generic_args()
                .find_map(|arg| match arg {
                    ast::GenericArg::TypeArg(arg) => {
                        arg.ty().map(super::super::context::type_label)
                    }
                    _ => None,
                })
        }
        _ => None,
    }
}
