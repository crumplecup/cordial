//! Path and receiver contracts used by expression evidence.
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasGenericArgs, HasName},
};
use tracing::instrument;

#[instrument(level = "trace", skip(path))]
pub(in crate::etiquettes::internal_error_chain::apply::graph_apply) fn names(
    path: ast::Path,
) -> Vec<String> {
    let mut names = path.qualifier().map(names).unwrap_or_default();
    if let Some(segment) = path.segment() {
        if let Some(name) = segment.name_ref() {
            names.push(name.text().to_string());
        } else if let Some(anchor) = segment.type_anchor() {
            if anchor.as_token().is_some() {
                if let Some(path) = anchor
                    .syntax()
                    .children()
                    .filter_map(ast::PathType::cast)
                    .last()
                    .and_then(|ty| ty.path())
                {
                    names.extend(self::names(path));
                }
            }
        }
    }
    names
}

#[instrument(level = "trace", skip(expr, name))]
pub(in crate::etiquettes::internal_error_chain::apply::graph_apply) fn bare_path(
    expr: &ast::Expr,
    name: &str,
) -> bool {
    matches!(expr, ast::Expr::PathExpr(expr) if expr.path().is_some_and(|path|
        path.qualifier().is_none() && path.coloncolon_token().is_none() && path.segment().is_some_and(|segment|
            segment.generic_arg_list().is_none() && segment.name_ref().is_some_and(|value| value.text() == name))))
}

#[instrument(level = "trace", skip(expr, field_name))]
pub(in crate::etiquettes::internal_error_chain::apply::graph_apply) fn self_field(
    expr: &ast::Expr,
    field_name: &str,
) -> bool {
    matches!(expr, ast::Expr::FieldExpr(field) if field.expr().is_some_and(|expr| bare_path(&expr, "self"))
        && field.name_ref().is_some_and(|name| name.text() == field_name))
}

#[instrument(level = "trace", skip(call, name))]
pub(in crate::etiquettes::internal_error_chain::apply::graph_apply) fn method(
    call: &ast::MethodCallExpr,
    name: &str,
) -> bool {
    call.name_ref().is_some_and(|value| value.text() == name)
        && call
            .arg_list()
            .is_some_and(|list| list.args().next().is_none())
}

#[instrument(level = "trace", skip(call))]
pub(in crate::etiquettes::internal_error_chain::apply::graph_apply) fn boxed_call(
    call: &ast::CallExpr,
    exact: bool,
) -> bool {
    if !call.arg_list().is_some_and(|list| list.args().count() == 1) {
        return false;
    }
    let Some(ast::Expr::PathExpr(expr)) = call.expr() else {
        return false;
    };
    let names = expr.path().map(names).unwrap_or_default();
    names.first().is_some_and(|name| name == "Box")
        && names.last().is_some_and(|name| name == "new")
        && (!exact || names.len() == 2)
}

#[instrument(level = "trace", skip(path, owner))]
pub(in crate::etiquettes::internal_error_chain::apply::graph_apply) fn record_name(
    path: Option<ast::Path>,
    owner: Option<&str>,
) -> Option<String> {
    let name = names(path?).pop()?;
    if name == "Self" {
        owner.map(str::to_string)
    } else {
        Some(name)
    }
}

#[instrument(level = "trace", skip(ty))]
pub(in crate::etiquettes::internal_error_chain::apply::graph_apply) fn type_name(
    ty: ast::Type,
) -> Option<String> {
    match ty {
        ast::Type::RefType(reference) => type_name(reference.ty()?),
        ast::Type::PathType(ty) => {
            let path = ty.path()?;
            if has_anchor(&path) {
                return None;
            }
            let names = names(path.clone());
            if names == ["Box"] {
                let arg = path.segment()?.generic_arg_list()?.generic_args().next()?;
                let ast::GenericArg::TypeArg(arg) = arg else {
                    return None;
                };
                type_name(arg.ty()?)
            } else {
                names.last().cloned()
            }
        }
        _ => None,
    }
}

#[instrument(level = "trace", skip(function))]
pub(in crate::etiquettes::internal_error_chain::apply::graph_apply) fn is_source_method(
    function: &ast::Fn,
) -> bool {
    if !function.name().is_some_and(|name| name.text() == "source") {
        return false;
    }
    let Some(item) = function
        .syntax()
        .parent()
        .filter(|node| ast::AssocItemList::can_cast(node.kind()))
        .and_then(|node| node.parent())
        .and_then(ast::Impl::cast)
    else {
        return false;
    };
    let Some(ast::Type::PathType(trait_type)) = item.trait_() else {
        return false;
    };
    trait_type.path().map(names).is_some_and(|names| {
        names == ["std", "error", "Error"] || names == ["core", "error", "Error"]
    })
}

#[instrument(level = "trace", skip(path))]
fn has_anchor(path: &ast::Path) -> bool {
    path.segment()
        .is_some_and(|segment| segment.type_anchor().is_some())
        || path.qualifier().is_some_and(|path| has_anchor(&path))
}
