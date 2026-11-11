//! Project conservative recording evidence from the database-issued syntax.

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasGenericArgs, HasName},
};
use tracing::instrument;

use crate::etiquettes::tracing::RECORDABLE_IDENTS;

/// The names the parameters bind, `self` first. Whether each can be recorded is
/// the database's verdict, not anything read from its written type.
#[instrument(level = "debug", skip(function))]
pub(super) fn parameter_names(function: &ast::Fn) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(list) = function.param_list() {
        if list.self_param().is_some() {
            names.push("self".into());
        }
        for pattern in list.params().filter_map(|param| param.pat()) {
            names.extend(
                pattern
                    .syntax()
                    .descendants()
                    .filter_map(ast::IdentPat::cast)
                    .filter_map(|binding| binding.name())
                    .map(|name| name.text().to_string()),
            );
        }
    }
    names
}

#[instrument(level = "debug", skip(ty))]
pub(super) fn unrecordable(ty: &ast::Type) -> bool {
    match ty {
        ast::Type::RefType(ty) => ty.ty().is_none_or(|ty| unrecordable(&ty)),
        ast::Type::PtrType(ty) => ty.ty().is_none_or(|ty| unrecordable(&ty)),
        ast::Type::ParenType(ty) => ty.ty().is_none_or(|ty| unrecordable(&ty)),
        ast::Type::ArrayType(ty) => ty.ty().is_none_or(|ty| unrecordable(&ty)),
        ast::Type::SliceType(ty) => ty.ty().is_none_or(|ty| unrecordable(&ty)),
        ast::Type::TupleType(ty) => ty.fields().any(|ty| unrecordable(&ty)),
        ast::Type::PathType(ty) => {
            let Some(segment) = ty.path().and_then(|path| path.segment()) else {
                return true;
            };
            if segment
                .name_ref()
                .is_none_or(|name| !RECORDABLE_IDENTS.contains(&name.text()))
            {
                return true;
            }
            if segment.parenthesized_arg_list().is_some() {
                return true;
            }
            segment
                .generic_arg_list()
                .into_iter()
                .flat_map(|list| list.generic_args())
                .any(|arg| match arg {
                    ast::GenericArg::TypeArg(arg) => arg.ty().is_none_or(|ty| unrecordable(&ty)),
                    ast::GenericArg::AssocTypeArg(arg) => {
                        arg.ty().is_some_and(|ty| unrecordable(&ty))
                    }
                    _ => false,
                })
        }
        _ => true,
    }
}

#[instrument(level = "trace", skip(ty))]
pub(super) fn borrowed(ty: &ast::Type) -> bool {
    ty.syntax()
        .descendants()
        .any(|node| ast::RefType::can_cast(node.kind()) || ast::PtrType::can_cast(node.kind()))
}

#[instrument(level = "trace", skip(ty))]
pub(super) fn returns_result(ty: &ast::Type) -> bool {
    match ty {
        ast::Type::PathType(ty) => ty
            .path()
            .and_then(|path| path.segment())
            .and_then(|segment| segment.name_ref())
            .is_some_and(|name| name.text().ends_with("Result")),
        ast::Type::RefType(ty) => ty.ty().is_some_and(|ty| returns_result(&ty)),
        ast::Type::ParenType(ty) => ty.ty().is_some_and(|ty| returns_result(&ty)),
        _ => false,
    }
}

#[instrument(level = "trace", skip(ty))]
pub(super) fn returns_self(ty: &ast::Type) -> bool {
    match ty {
        ast::Type::PathType(ty) => ty.path().is_some_and(|path| {
            path.qualifier().is_none()
                && path
                    .segment()
                    .and_then(|segment| segment.name_ref())
                    .is_some_and(|name| name.text() == "Self")
        }),
        ast::Type::RefType(ty) => ty.ty().is_some_and(|ty| returns_self(&ty)),
        ast::Type::ParenType(ty) => ty.ty().is_some_and(|ty| returns_self(&ty)),
        _ => false,
    }
}

#[instrument(level = "trace", skip(ty))]
pub(super) fn contains_bool(ty: &ast::Type) -> bool {
    match ty {
        ast::Type::PathType(_) => ty
            .syntax()
            .descendants()
            .filter_map(ast::PathSegment::cast)
            .filter_map(|segment| segment.name_ref())
            .any(|name| name.text() == "bool"),
        ast::Type::RefType(ty) => ty.ty().is_some_and(|ty| contains_bool(&ty)),
        ast::Type::ParenType(ty) => ty.ty().is_some_and(|ty| contains_bool(&ty)),
        ast::Type::TupleType(ty) => ty.fields().any(|ty| contains_bool(&ty)),
        _ => false,
    }
}
