//! Shared expression helpers and getter field-read classification.

use syn::{Block, Expr, Stmt};
use tracing::instrument;

/// How a getter body reads a field; each maps to a derive option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::etiquettes::derives) enum FieldRead {
    /// `&self.field` maps to a plain `#[derive(Getters)]`.
    Direct,
    /// Bare `self.field` needs `#[getter(copy)]`.
    DirectOwned,
    Clone,
    AsStr,
    AsRef,
}

#[instrument(level = "debug", skip(block), ret)]
pub(in crate::etiquettes::derives) fn classify_field_read(
    block: &Block,
) -> Option<(String, FieldRead)> {
    let stmts = non_item_stmts(block);
    if stmts.len() != 1 {
        return None;
    }
    expr_field_read(stmt_tail_expr(stmts[0])?)
}

#[instrument(level = "debug", skip(expr), ret)]
fn expr_field_read(expr: &Expr) -> Option<(String, FieldRead)> {
    match expr {
        Expr::Reference(reference) => {
            let (name, _) = expr_field_read(&reference.expr)?;
            Some((name, FieldRead::Direct))
        }
        Expr::Return(return_expr) => return_expr
            .expr
            .as_ref()
            .and_then(|inner| expr_field_read(inner)),
        Expr::Paren(paren) => expr_field_read(&paren.expr),
        Expr::Group(group) => expr_field_read(&group.expr),
        Expr::Field(field) => {
            let name = field_member_name(&field.member)?;
            expr_is_self(&field.base).then_some((name, FieldRead::DirectOwned))
        }
        Expr::MethodCall(call) if call.args.is_empty() => {
            let kind = match call.method.to_string().as_str() {
                "clone" | "to_owned" => FieldRead::Clone,
                "as_str" => FieldRead::AsStr,
                "as_ref" => FieldRead::AsRef,
                _ => return None,
            };
            let (field, inner) = expr_field_read(&call.receiver)?;
            matches!(inner, FieldRead::Direct | FieldRead::DirectOwned).then_some((field, kind))
        }
        _ => None,
    }
}

#[instrument(level = "debug", skip(expr), ret)]
pub(super) fn expr_is_self_field(expr: &Expr, field_name: &str) -> bool {
    match expr {
        Expr::Field(field) => {
            field_member_name(&field.member).as_deref() == Some(field_name)
                && expr_is_self(&field.base)
        }
        Expr::Paren(paren) => expr_is_self_field(&paren.expr, field_name),
        Expr::Group(group) => expr_is_self_field(&group.expr, field_name),
        _ => false,
    }
}

#[instrument(level = "debug", skip(expr), ret)]
pub(super) fn expr_is_self(expr: &Expr) -> bool {
    match expr {
        Expr::Path(path) => path.path.is_ident("self"),
        Expr::Paren(paren) => expr_is_self(&paren.expr),
        Expr::Group(group) => expr_is_self(&group.expr),
        _ => false,
    }
}

#[instrument(level = "debug", skip(block))]
pub(super) fn non_item_stmts(block: &Block) -> Vec<&Stmt> {
    block
        .stmts
        .iter()
        .filter(|stmt| !matches!(stmt, Stmt::Item(_)))
        .collect()
}

#[instrument(level = "debug", skip(stmt))]
pub(super) fn stmt_tail_expr(stmt: &Stmt) -> Option<&Expr> {
    match stmt {
        Stmt::Expr(expr, _) => Some(expr),
        _ => None,
    }
}

#[instrument(level = "debug", skip(member))]
pub(super) fn field_member_name(member: &syn::Member) -> Option<String> {
    match member {
        syn::Member::Named(ident) => Some(ident.to_string()),
        syn::Member::Unnamed(_) => None,
    }
}
