//! Constructor and struct-literal body classification.

use syn::{Block, Expr, FnArg, Pat, Signature};
use tracing::instrument;

use super::exprs::{field_member_name, non_item_stmts, stmt_tail_expr};

#[instrument(level = "debug", skip(block))]
pub(in crate::etiquettes::derives) fn body_is_struct_literal(
    block: &Block,
    type_name: &str,
) -> bool {
    let stmts = non_item_stmts(block);
    if stmts.is_empty() || stmts.len() > 2 {
        return false;
    }
    stmts.iter().any(|stmt| {
        stmt_tail_expr(stmt).is_some_and(|expr| expr_is_struct_literal(expr, type_name))
    })
}

/// Every field in the constructor's `Self { .. }` literal must be a
/// trivial pass-through of a same-named parameter: bare `field`,
/// `field.into()`, `field.clone()`, or `field.to_owned()`.
#[instrument(level = "debug", skip(sig, block), ret)]
pub(in crate::etiquettes::derives) fn constructor_fields_match_params(
    sig: &Signature,
    block: &Block,
) -> bool {
    let params: std::collections::HashSet<String> = sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(pat_type) => pat_ident_name(&pat_type.pat),
            FnArg::Receiver(_) => None,
        })
        .collect();

    let Some(item) = struct_literal_expr(block) else {
        return false;
    };
    if item.fields.len() != params.len() {
        return false;
    }
    item.fields.iter().all(|field_value| {
        let Some(field_name) = field_member_name(&field_value.member) else {
            return false;
        };
        params.contains(&field_name) && field_expr_matches_param(&field_value.expr, &field_name)
    })
}

#[instrument(level = "debug", skip(pat))]
fn pat_ident_name(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(ident) => Some(ident.ident.to_string()),
        _ => None,
    }
}

#[instrument(level = "debug", skip(block))]
fn struct_literal_expr(block: &Block) -> Option<&syn::ExprStruct> {
    non_item_stmts(block)
        .into_iter()
        .find_map(|stmt| struct_literal_from_expr(stmt_tail_expr(stmt)?))
}

#[instrument(level = "debug", skip(expr))]
fn struct_literal_from_expr(expr: &Expr) -> Option<&syn::ExprStruct> {
    match expr {
        Expr::Struct(item) => Some(item),
        Expr::Return(return_expr) => struct_literal_from_expr(return_expr.expr.as_ref()?),
        _ => None,
    }
}

#[instrument(level = "debug", skip(expr))]
fn field_expr_matches_param(expr: &Expr, param_name: &str) -> bool {
    match expr {
        Expr::Path(path) => path.path.is_ident(param_name),
        Expr::MethodCall(call) if call.args.is_empty() => {
            matches!(
                call.method.to_string().as_str(),
                "into" | "clone" | "to_owned"
            ) && matches!(&*call.receiver, Expr::Path(path) if path.path.is_ident(param_name))
        }
        _ => false,
    }
}

#[instrument(level = "debug", skip(expr))]
fn expr_is_struct_literal(expr: &Expr, type_name: &str) -> bool {
    match expr {
        Expr::Struct(item) => type_matches(&item.path, type_name) || path_is_self(&item.path),
        Expr::Return(return_expr) => return_expr
            .expr
            .as_ref()
            .is_some_and(|inner| expr_is_struct_literal(inner, type_name)),
        _ => false,
    }
}

#[instrument(level = "debug", skip(path))]
fn path_is_self(path: &syn::Path) -> bool {
    path.is_ident("Self")
}

#[instrument(level = "debug", skip(path))]
fn type_matches(path: &syn::Path, type_name: &str) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == type_name)
}
