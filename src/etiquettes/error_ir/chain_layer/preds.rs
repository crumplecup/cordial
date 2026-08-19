//! Predicates and labels used by [`super::ChainLayer`].

use syn::{
    Expr, ExprClosure, ExprMethodCall, ExprPath, GenericArgument, Member, PathArguments,
    ReturnType, Type, TypePath,
};

use super::super::visitor::{raw_expr_snippet, truncate_snippet};
use crate::etiquettes::error_sites::infer_foreign_error_type;

use tracing::instrument;

#[instrument(level = "debug", skip(expr))]
fn chain_expr_snippet(expr: &Expr) -> String {
    truncate_snippet(&raw_expr_snippet(expr), 96)
}

#[instrument(level = "debug", skip(output))]
pub(super) fn return_type_label(output: &ReturnType) -> Option<String> {
    match output {
        ReturnType::Default => None,
        ReturnType::Type(_, ty) => Some(type_label(ty)),
    }
}

#[instrument(level = "debug", skip(try_expr))]
pub(super) fn try_propagates_into_umbrella(
    fn_return_type: &Option<String>,
    try_expr: &Expr,
) -> bool {
    if is_option_try_expr(try_expr) {
        return false;
    }
    return_type_is_umbrella(fn_return_type)
}

#[instrument(level = "debug")]
pub(super) fn return_type_is_umbrella(fn_return_type: &Option<String>) -> bool {
    let Some(return_type) = fn_return_type else {
        return false;
    };
    if return_type.starts_with("Option") || return_type.contains("Option<") {
        return false;
    }
    if is_foreign_result_return_type(return_type) {
        return false;
    }
    return_type.contains("Result")
}

#[instrument(level = "trace", skip(expr), ret)]
fn is_option_try_expr(expr: &Expr) -> bool {
    matches!(expr, Expr::MethodCall(call) if call.method == "ok")
}

#[instrument(level = "trace")]
fn is_foreign_result_return_type(return_type: &str) -> bool {
    if return_type.contains("io::Result") {
        return true;
    }
    for foreign in [
        "std::io::Error",
        "serde_json::Error",
        "syn::Error",
        "csv::Error",
        "cargo_metadata::Error",
    ] {
        if return_type.contains(foreign) {
            return true;
        }
    }
    false
}

#[instrument(level = "debug", skip(expr))]
pub(super) fn foreign_try_site(expr: &Expr) -> Option<(String, String)> {
    if expr_contains_map_err(expr) {
        return None;
    }
    let source_snippet = chain_expr_snippet(expr);
    let (foreign_type, _, _) = infer_foreign_error_type(&source_snippet)?;
    Some((foreign_type, source_snippet))
}

#[instrument(level = "debug", skip(call, receiver))]
pub(super) fn preserved_map_err_conversion(
    call: &ExprMethodCall,
    receiver: &Expr,
) -> Option<(String, String)> {
    let converter = call.args.first()?;
    if map_err_stringifies(converter) {
        return None;
    }
    if !map_err_preserves_chain(converter) {
        return None;
    }
    let source_snippet = chain_expr_snippet(receiver);
    let (foreign_type, _, _) = infer_foreign_error_type(&source_snippet)?;
    Some((foreign_type, source_snippet))
}

#[instrument(level = "debug")]
pub(super) fn extract_from_source_type(trait_path: &syn::Path) -> Option<&Type> {
    let segment = trait_path.segments.last()?;
    if segment.ident != "From" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    let GenericArgument::Type(from_type) = args.args.first()? else {
        return None;
    };
    Some(from_type)
}

#[instrument(level = "debug", skip(expr))]
fn map_err_stringifies(expr: &Expr) -> bool {
    chain_expr_contains_to_string(expr)
}

#[instrument(level = "debug", skip(expr))]
fn map_err_preserves_chain(expr: &Expr) -> bool {
    match expr {
        // `map_err(CrateError::from)` and `map_err(CrateError::cargo_metadata)` are
        // the preferred 1-arg wrap: a constructor that keeps the foreign error.
        Expr::Path(path) => {
            path_is_into_or_from(path) || !path_constructor_drops_typed_source(path)
        }
        Expr::Call(call) => path_is_into_or_from_fn(&call.func),
        Expr::Closure(closure) => closure_preserves_chain(closure),
        _ => false,
    }
}

#[instrument(level = "debug", skip(path))]
fn path_is_into_or_from(path: &ExprPath) -> bool {
    let segments = &path.path.segments;
    if segments.len() == 2 && segments[0].ident == "Into" && segments[1].ident == "into" {
        return true;
    }
    segments
        .last()
        .is_some_and(|segment| segment.ident == "from")
}

#[instrument(level = "debug", skip(func))]
fn path_is_into_or_from_fn(func: &Expr) -> bool {
    match func {
        Expr::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "from"),
        _ => false,
    }
}

#[instrument(level = "debug", skip(closure))]
fn closure_preserves_chain(closure: &ExprClosure) -> bool {
    if chain_expr_contains_to_string(&closure.body) {
        return false;
    }
    expr_contains_source_field(&closure.body) || expr_forwards_error_binding(&closure.body)
}

#[instrument(level = "debug", skip(expr))]
fn expr_forwards_error_binding(expr: &Expr) -> bool {
    match expr {
        Expr::Call(call) if path_is_into_or_from_fn(&call.func) => true,
        Expr::Call(call)
            if !constructor_drops_typed_source(&call.func)
                && call.args.iter().any(expr_is_error_binding) =>
        {
            true
        }
        Expr::Path(path) => path_is_into_or_from(path),
        Expr::Block(block) => block.block.stmts.last().is_some_and(|stmt| match stmt {
            syn::Stmt::Expr(inner, _) => expr_forwards_error_binding(inner),
            _ => false,
        }),
        Expr::Paren(paren) => expr_forwards_error_binding(&paren.expr),
        Expr::Group(group) => expr_forwards_error_binding(&group.expr),
        _ => false,
    }
}

#[instrument(level = "debug", skip(func))]
fn constructor_drops_typed_source(func: &Expr) -> bool {
    let Expr::Path(path) = func else {
        return false;
    };
    path_constructor_drops_typed_source(path)
}

#[instrument(level = "debug", skip(path))]
fn path_constructor_drops_typed_source(path: &ExprPath) -> bool {
    path.path.segments.last().is_some_and(|segment| {
        matches!(
            segment.ident.to_string().as_str(),
            "invariant" | "to_string" | "from_str"
        )
    })
}

#[instrument(level = "debug", skip(expr))]
fn expr_is_error_binding(expr: &Expr) -> bool {
    match expr {
        Expr::Path(path) => path
            .path
            .get_ident()
            .is_some_and(|ident| ident == "e" || ident == "err" || ident == "error"),
        Expr::Paren(paren) => expr_is_error_binding(&paren.expr),
        _ => false,
    }
}

#[instrument(level = "debug", skip(expr))]
fn chain_expr_contains_to_string(expr: &Expr) -> bool {
    match expr {
        Expr::MethodCall(call) if call.method == "to_string" => {
            expr_is_error_binding(&call.receiver)
        }
        Expr::Call(call) => {
            chain_expr_contains_to_string(&call.func)
                || call.args.iter().any(chain_expr_contains_to_string)
        }
        Expr::MethodCall(call) => {
            chain_expr_contains_to_string(&call.receiver)
                || call.args.iter().any(chain_expr_contains_to_string)
        }
        Expr::Closure(closure) => chain_expr_contains_to_string(&closure.body),
        Expr::Struct(item) => item
            .fields
            .iter()
            .any(|field| chain_expr_contains_to_string(&field.expr)),
        Expr::Field(field) => chain_expr_contains_to_string(&field.base),
        Expr::Paren(paren) => chain_expr_contains_to_string(&paren.expr),
        Expr::Group(group) => chain_expr_contains_to_string(&group.expr),
        Expr::Block(block) => block.block.stmts.iter().any(|stmt| match stmt {
            syn::Stmt::Expr(inner, _) => chain_expr_contains_to_string(inner),
            _ => false,
        }),
        _ => false,
    }
}

#[instrument(level = "debug", skip(expr))]
fn expr_contains_source_field(expr: &Expr) -> bool {
    match expr {
        Expr::Struct(item) => item.fields.iter().any(|field| {
            matches!(
                &field.member,
                Member::Named(ident) if ident == "source" || ident == "err"
            )
        }),
        Expr::Call(call) => {
            expr_contains_source_field(&call.func)
                || call.args.iter().any(expr_contains_source_field)
        }
        Expr::MethodCall(call) => {
            expr_contains_source_field(&call.receiver)
                || call.args.iter().any(expr_contains_source_field)
        }
        Expr::Closure(closure) => expr_contains_source_field(&closure.body),
        Expr::Field(field) => expr_contains_source_field(&field.base),
        Expr::Paren(paren) => expr_contains_source_field(&paren.expr),
        Expr::Group(group) => expr_contains_source_field(&group.expr),
        _ => false,
    }
}

#[instrument(level = "debug", skip(expr))]
pub(super) fn expr_contains_map_err(expr: &Expr) -> bool {
    match expr {
        Expr::MethodCall(call) if call.method == "map_err" => true,
        Expr::Try(try_expr) => expr_contains_map_err(&try_expr.expr),
        Expr::Call(call) => {
            expr_contains_map_err(&call.func) || call.args.iter().any(expr_contains_map_err)
        }
        Expr::MethodCall(call) => {
            expr_contains_map_err(&call.receiver) || call.args.iter().any(expr_contains_map_err)
        }
        Expr::Field(field) => expr_contains_map_err(&field.base),
        Expr::Paren(paren) => expr_contains_map_err(&paren.expr),
        Expr::Group(group) => expr_contains_map_err(&group.expr),
        _ => false,
    }
}

#[instrument(level = "trace", skip(ty), ret)]
pub(super) fn is_foreign_rust_type(ty: &Type) -> bool {
    let label = type_label(ty);
    [
        "std::",
        "serde_json::",
        "serde_yaml::",
        "syn::",
        "csv::",
        "cargo_metadata::",
        "reqwest::",
        "url::",
        "toml::",
        "walkdir::",
        "tempfile::",
    ]
    .iter()
    .any(|prefix| label.starts_with(prefix))
        || label.ends_with("Error") && label.contains("::") && !label.ends_with("ErrorKind")
}

#[instrument(level = "debug")]
pub(super) fn enum_name_suggests_error_kind(name: &str) -> bool {
    name.ends_with("ErrorKind") || name.ends_with("Kind")
}

#[instrument(level = "debug", skip(ty))]
pub(super) fn foreign_type_from_rust_type(ty: &Type) -> Option<String> {
    let label = type_label(ty);
    if label == "String" || label == "?" {
        return None;
    }
    Some(label)
}

#[instrument(level = "debug", skip(ty))]
pub(super) fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => type_path
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}

#[instrument(level = "trace", skip(ty))]
pub(super) fn is_string_type(ty: &Type) -> bool {
    match ty {
        Type::Path(TypePath { path, .. }) => path.is_ident("String"),
        Type::Reference(reference) => is_string_type(&reference.elem),
        Type::Paren(paren) => is_string_type(&paren.elem),
        Type::Group(group) => is_string_type(&group.elem),
        _ => false,
    }
}
