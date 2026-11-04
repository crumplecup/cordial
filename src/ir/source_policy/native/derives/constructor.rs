//! Constructor pass-through shape, independent of policy thresholds.
use super::exprs;
use ra_ap_syntax::ast;
use std::collections::HashSet;

#[tracing::instrument(level = "trace", skip(method, stmts))]
pub(super) fn arity(method: &ast::Fn, stmts: &[Option<ast::Expr>], ty: &str) -> Option<usize> {
    if method.ret_type().is_none() || stmts.is_empty() || stmts.len() > 2 {
        return None;
    }
    if !stmts
        .iter()
        .flatten()
        .cloned()
        .filter_map(record)
        .any(|record| {
            record
                .path()
                .and_then(|path| path.segment())
                .and_then(|part| part.name_ref())
                .is_some_and(|name| name.text() == "Self" || name.text() == ty)
        })
    {
        return None;
    }
    let record = stmts.iter().flatten().cloned().find_map(record)?;
    let fields: Vec<_> = record.record_expr_field_list()?.fields().collect();
    let params: HashSet<_> = exprs::params(method).into_iter().collect();
    if fields.len() != params.len()
        || !fields.iter().all(|field| {
            let Some(name) = field.field_name() else {
                return false;
            };
            let name = name.text().to_string();
            params.contains(&name)
                && match field.expr() {
                    Some(expr) => matches_param(expr, &name),
                    None => field.colon_token().is_none(),
                }
        })
    {
        return None;
    }
    Some(method.param_list()?.params().count())
}

#[tracing::instrument(level = "trace", skip(expr))]
fn record(expr: ast::Expr) -> Option<ast::RecordExpr> {
    match expr {
        ast::Expr::RecordExpr(expr) => Some(expr),
        ast::Expr::ReturnExpr(expr) => record(expr.expr()?),
        _ => None,
    }
}

#[tracing::instrument(level = "trace", skip(expr))]
fn matches_param(expr: ast::Expr, param: &str) -> bool {
    match expr {
        ast::Expr::MethodCallExpr(call)
            if exprs::no_arguments(&call)
                && call
                    .name_ref()
                    .is_some_and(|name| matches!(name.text(), "into" | "clone" | "to_owned")) =>
        {
            call.receiver()
                .is_some_and(|expr| exprs::bare_path(&expr).as_deref() == Some(param))
        }
        other => exprs::bare_path(&other).as_deref() == Some(param),
    }
}
