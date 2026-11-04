//! Method body shapes from existing AST nodes.
use crate::ir::DeriveFieldRead as FieldRead;
use ra_ap_syntax::ast::{self, HasArgList};

/// Preserve let/macro statements as non-expressions, and ignore item statements.
#[tracing::instrument(level = "trace", skip(body))]
pub(super) fn statements(body: &ast::BlockExpr) -> Vec<Option<ast::Expr>> {
    let Some(list) = body.stmt_list() else {
        return Vec::new();
    };
    list.statements()
        .filter_map(|stmt| match stmt {
            ast::Stmt::Item(_) => None,
            ast::Stmt::ExprStmt(stmt) => Some(stmt.expr()),
            _ => Some(None),
        })
        .chain(list.tail_expr().map(Some))
        .collect()
}

#[tracing::instrument(level = "trace", skip(expr))]
pub(super) fn field_read(expr: ast::Expr) -> Option<(String, FieldRead)> {
    match expr {
        ast::Expr::RefExpr(expr) => {
            field_read(expr.expr()?).map(|(name, _)| (name, FieldRead::Direct))
        }
        ast::Expr::ReturnExpr(expr) => field_read(expr.expr()?),
        ast::Expr::ParenExpr(expr) => field_read(expr.expr()?),
        ast::Expr::FieldExpr(expr) => is_self(expr.expr()?)
            .then(|| {
                expr.name_ref()
                    .filter(|name| name.int_number_token().is_none())
                    .map(|name| (name.text().to_string(), FieldRead::DirectOwned))
            })
            .flatten(),
        ast::Expr::MethodCallExpr(call) if no_arguments(&call) => {
            let kind = match call.name_ref()?.text() {
                "clone" | "to_owned" => FieldRead::Clone,
                "as_str" => FieldRead::AsStr,
                "as_ref" => FieldRead::AsRef,
                _ => return None,
            };
            let (field, inner) = field_read(call.receiver()?)?;
            matches!(inner, FieldRead::Direct | FieldRead::DirectOwned).then_some((field, kind))
        }
        _ => None,
    }
}

#[tracing::instrument(level = "trace", skip(expr))]
pub(super) fn is_self(expr: ast::Expr) -> bool {
    match expr {
        ast::Expr::ParenExpr(expr) => expr.expr().is_some_and(is_self),
        other => bare_path(&other).as_deref() == Some("self"),
    }
}

#[tracing::instrument(level = "trace", skip(expr))]
pub(super) fn self_field(expr: ast::Expr, name: &str) -> bool {
    match expr {
        ast::Expr::ParenExpr(expr) => expr.expr().is_some_and(|expr| self_field(expr, name)),
        ast::Expr::FieldExpr(expr) => {
            expr.name_ref()
                .is_some_and(|field| field.int_number_token().is_none() && field.text() == name)
                && expr.expr().is_some_and(is_self)
        }
        _ => false,
    }
}

#[tracing::instrument(level = "trace", skip(expr))]
pub(super) fn bare_path(expr: &ast::Expr) -> Option<String> {
    let ast::Expr::PathExpr(expr) = expr else {
        return None;
    };
    let path = expr.path()?;
    let segment = path.segment()?;
    (path.qualifier().is_none() && segment.coloncolon_token().is_none())
        .then(|| segment.name_ref().map(|name| name.text().to_string()))
        .flatten()
}

#[tracing::instrument(level = "trace", skip(owner))]
pub(super) fn no_arguments(owner: &impl HasArgList) -> bool {
    owner
        .arg_list()
        .is_some_and(|args| args.args().next().is_none())
}

#[tracing::instrument(level = "trace", skip(method))]
pub(super) fn params(method: &ast::Fn) -> Vec<String> {
    method
        .param_list()
        .into_iter()
        .flat_map(|params| params.params())
        .filter_map(|param| match param.pat()? {
            ast::Pat::IdentPat(pat) => {
                ra_ap_syntax::ast::HasName::name(&pat).map(|name| name.text().to_string())
            }
            _ => None,
        })
        .collect()
}
