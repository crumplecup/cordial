//! Exact storage and edit facts for all named and tuple error fields.
use super::super::{GraphField, GraphFields};
use super::types::{allowed_attrs, is_plain, lifetime_name, path_parts, type_name, visibility};
use crate::etiquettes::internal_error_chain::apply::{
    graph_apply::Shape,
    inventory::{ErrorEdgeEvidence, ErrorEdgeField, StringStorage},
};
use crate::loader::DatabaseSource;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasGenericArgs, HasName, HasVisibility},
};

#[tracing::instrument(level = "trace", skip(file, fields))]
pub(super) fn collect(file: &DatabaseSource<'_>, fields: Option<ast::FieldList>) -> GraphFields {
    match fields {
        None => GraphFields::Unit,
        Some(ast::FieldList::RecordFieldList(fields)) => GraphFields::Named(
            fields
                .fields()
                .filter_map(|field| {
                    collect_field(
                        file,
                        &field,
                        field.ty()?,
                        field.name().map(|name| name.text().to_string()),
                        field.visibility(),
                    )
                })
                .collect(),
        ),
        Some(ast::FieldList::TupleFieldList(fields)) => GraphFields::Unnamed(
            fields
                .fields()
                .filter_map(|field| {
                    collect_field(file, &field, field.ty()?, None, field.visibility())
                })
                .collect(),
        ),
    }
}

#[tracing::instrument(level = "trace", skip(file, field, ty, vis))]
fn collect_field(
    file: &DatabaseSource<'_>,
    field: &impl HasAttrs,
    ty: ast::Type,
    name: Option<String>,
    vis: Option<ast::Visibility>,
) -> Option<GraphField> {
    let type_range = ty.syntax().text_range();
    let borrowed_referent = borrowed_referent(file, &ty);
    let owned_referent = if let ast::Type::RefType(reference) = &ty {
        Some(file.text(reference.ty()?.syntax()).to_string())
    } else {
        None
    };
    let reference = if let ast::Type::RefType(reference) = &ty {
        Some(reference)
    } else {
        None
    };
    let cfg_attrs = field
        .attrs()
        .filter(|attr| attr.simple_name().as_deref() == Some("cfg"))
        .map(|attr| file.text(attr.syntax()).to_string())
        .collect();
    let copy_shape = reference.is_some()
        || [
            "bool", "char", "u8", "u16", "u32", "u64", "usize", "i8", "i16", "i32", "i64", "isize",
        ]
        .iter()
        .any(|name| is_plain(&ty, name));
    let unit_reference = reference.is_some_and(|reference| {
        matches!(reference.ty(), Some(ast::Type::TupleType(tuple)) if tuple.fields().next().is_none())
    });
    let source_edge = (name.is_none() || name.as_deref() == Some("source"))
        .then(|| ErrorEdgeEvidence::collect_native(file, &ty, ErrorEdgeField::Source))
        .flatten();
    let kind_edge = (name.as_deref() == Some("kind"))
        .then(|| ErrorEdgeEvidence::collect_native(file, &ty, ErrorEdgeField::Kind))
        .flatten();
    let candidate_storage = if is_plain(&ty, "String") {
        Some(StringStorage::Owned)
    } else {
        reference
            .filter(|reference| reference.ty().is_some_and(|ty| is_plain(&ty, "str")))
            .map(|reference| StringStorage::Borrowed {
                lifetime: reference
                    .lifetime()
                    .map(|lifetime| lifetime_name(&lifetime)),
                mutable: reference.mut_token().is_some(),
            })
    };
    Some(GraphField {
        name,
        line: super::types::start_line(file, field.syntax()),
        type_line: file.line(type_range.start()) as usize,
        visibility: visibility(vis),
        allowed_attrs: allowed_attrs(field),
        cfg_attrs,
        type_name: type_name(&ty),
        storage: storage_shape(&ty),
        copy_shape,
        reference: reference.is_some(),
        unit_reference,
        type_range: Some((type_range.start().into(), type_range.end().into())),
        borrowed_referent,
        owned_referent,
        stripped_lifetime: stripped_lifetime(file, &ty),
        source_edge,
        kind_edge,
        candidate_storage,
        getter_storage: None,
    })
}

#[tracing::instrument(level = "trace", skip(ty))]
fn storage_shape(ty: &ast::Type) -> Option<(Shape, String)> {
    if is_plain(ty, "String") {
        return Some((Shape::String, "str".into()));
    }
    if let ast::Type::RefType(reference) = ty {
        if reference.mut_token().is_some() {
            return None;
        }
        let name = type_name(&reference.ty()?)?;
        return Some((
            if name == "str" {
                Shape::String
            } else {
                Shape::Value
            },
            name,
        ));
    }
    let parts = path_parts(ty)?;
    if parts.len() == 1 && parts[0].name_ref()?.text() == "Box" {
        let args: Vec<_> = parts[0].generic_arg_list()?.generic_args().collect();
        let [ast::GenericArg::TypeArg(inner)] = args.as_slice() else {
            return None;
        };
        return Some((Shape::Boxed, type_name(&inner.ty()?)?));
    }
    Some((Shape::Value, type_name(ty)?))
}

#[tracing::instrument(level = "trace", skip(file, ty))]
fn stripped_lifetime(file: &DatabaseSource<'_>, ty: &ast::Type) -> Option<(String, String)> {
    let ast::Type::RefType(reference) = ty else {
        return None;
    };
    let inner = reference.ty()?;
    let parts = path_parts(&inner)?;
    let arguments = parts.last()?.generic_arg_list()?;
    let args: Vec<_> = arguments.generic_args().collect();
    let [ast::GenericArg::LifetimeArg(lifetime)] = args.as_slice() else {
        return None;
    };
    let args = arguments.syntax().text_range();
    let range = inner.syntax().text_range();
    Some((
        lifetime_name(&lifetime.lifetime()?),
        format!(
            "{}{}",
            &file.source()[usize::from(range.start())..usize::from(args.start())],
            &file.source()[usize::from(args.end())..usize::from(range.end())]
        ),
    ))
}

#[tracing::instrument(level = "trace", skip(file, ty))]
fn borrowed_referent(file: &DatabaseSource<'_>, ty: &ast::Type) -> Option<String> {
    if is_plain(ty, "String") {
        Some("str".into())
    } else if let Some(parts) = path_parts(ty)
        && parts.len() == 1
        && parts[0].name_ref()?.text() == "Box"
    {
        let ast::GenericArg::TypeArg(inner) = parts[0].generic_arg_list()?.generic_args().next()?
        else {
            return None;
        };
        Some(file.text(inner.ty()?.syntax()).to_string())
    } else {
        Some(file.text(ty.syntax()).to_string())
    }
}
