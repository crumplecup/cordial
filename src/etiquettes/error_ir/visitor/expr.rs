//! Expression helpers shared by the sites visitor and gated layers.

use syn::{Expr, ExprCall, Pat, Type};

// --- sites helpers (shared: also used by chain/compliance snippet rendering) ---

use tracing::instrument;
#[instrument(level = "debug", skip(expr))]
pub(super) fn sites_err_payload(expr: &Expr) -> Option<&Expr> {
    let Expr::Call(ExprCall { func, args, .. }) = expr else {
        return None;
    };
    if !expr_is_err(func) {
        return None;
    }
    args.first()
}

#[instrument(level = "debug", skip(expr))]
fn expr_is_err(expr: &Expr) -> bool {
    match expr {
        Expr::Path(path) => path.path.is_ident("Err"),
        _ => false,
    }
}

#[instrument(level = "debug", skip(expr))]
pub(super) fn if_let_err_source(expr: &Expr) -> Option<&Expr> {
    let Expr::Let(let_expr) = expr else {
        return None;
    };
    if !pat_is_err(&let_expr.pat) {
        return None;
    }
    Some(&let_expr.expr)
}

#[instrument(level = "debug", skip(pat))]
pub fn pat_is_err(pat: &Pat) -> bool {
    match pat {
        Pat::TupleStruct(tuple) => tuple.path.is_ident("Err"),
        Pat::Path(path) => path.path.is_ident("Err"),
        _ => false,
    }
}

#[instrument(level = "debug", skip(arms))]
pub(super) fn match_has_err_arm(arms: &[syn::Arm]) -> bool {
    arms.iter().any(|arm| pat_is_err(&arm.pat))
}

#[instrument(level = "debug", skip(expr))]
pub(super) fn sites_expr_snippet(expr: &Expr) -> String {
    truncate_snippet(&raw_expr_snippet(expr), 72)
}

// --- shared helpers (used across sites/chain/compliance) ---

#[instrument(level = "debug", skip(expr))]
pub fn raw_expr_snippet(expr: &Expr) -> String {
    match expr {
        Expr::Macro(mac) => format!("{}!(…)", macro_path_label(&mac.mac.path)),
        Expr::Call(call) => format!("{}(…)", expr_path_label(&call.func)),
        Expr::MethodCall(method) => {
            format!(
                "{}.{}(…)",
                raw_expr_snippet(&method.receiver),
                method.method
            )
        }
        Expr::Path(path) => path
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        Expr::Field(field) => format!(
            "{}.{}",
            raw_expr_snippet(&field.base),
            member_label(&field.member)
        ),
        Expr::Try(try_expr) => format!("{}?", raw_expr_snippet(&try_expr.expr)),
        _ => "…".to_string(),
    }
}

#[instrument(level = "debug", skip(expr))]
fn expr_path_label(expr: &Expr) -> String {
    match expr {
        Expr::Path(path) => path
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        Expr::Field(field) => format!(
            "{}.{}",
            raw_expr_snippet(&field.base),
            member_label(&field.member)
        ),
        _ => "…".to_string(),
    }
}

#[instrument(level = "debug", skip(path))]
fn macro_path_label(path: &syn::Path) -> String {
    path.get_ident()
        .map(syn::Ident::to_string)
        .unwrap_or_else(|| path_label(path))
}

#[instrument(level = "debug", skip(member))]
fn member_label(member: &syn::Member) -> String {
    match member {
        syn::Member::Named(ident) => ident.to_string(),
        syn::Member::Unnamed(index) => index.index.to_string(),
    }
}

#[instrument(level = "debug")]
pub fn truncate_snippet(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    format!("{truncated}…")
}

#[instrument(level = "debug", skip(ty))]
pub(super) fn impl_type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => path_label(&type_path.path),
        Type::Reference(reference) => impl_type_label(&reference.elem),
        Type::Paren(paren) => impl_type_label(&paren.elem),
        Type::Group(group) => impl_type_label(&group.elem),
        _ => "?".to_string(),
    }
}

#[instrument(level = "debug", skip(path))]
pub(super) fn path_label(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_else(|| "?".to_string())
}
