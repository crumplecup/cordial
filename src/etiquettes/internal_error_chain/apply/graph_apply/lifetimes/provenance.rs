//! Conservative constructor and forwarding-return lifetime origins.
use crate::etiquettes::internal_error_chain::apply::graph_apply::syntax;
use ra_ap_syntax::ast::{self, HasArgList, HasGenericArgs, HasName};
use std::collections::BTreeSet;
use tracing::instrument;

#[instrument(level = "trace", skip(item))]
fn sole_tail(item: &ast::Fn) -> Option<ast::Expr> {
    let body = item.body()?.stmt_list()?;
    body.statements()
        .next()
        .is_none()
        .then(|| body.tail_expr())
        .flatten()
}

#[instrument(level = "trace", skip(item, parameters))]
pub(super) fn safe_constructor(item: &ast::Fn, parameters: &BTreeSet<String>) -> bool {
    if parameters.is_empty() {
        return false;
    }
    match sole_tail(item) {
        Some(ast::Expr::RecordExpr(record)) => {
            if !record.path().is_some_and(|path| {
                path.qualifier().is_none()
                    && path.coloncolon_token().is_none()
                    && path.segment().is_some_and(|segment| {
                        segment.generic_arg_list().is_none()
                            && segment.name_ref().is_some_and(|name| name.text() == "Self")
                    })
            }) {
                return false;
            }
            let Some(fields) = record.record_expr_field_list() else {
                return false;
            };
            fields.dotdot_token().is_none()
                && fields.fields().all(|field| {
                    let expr = field.expr();
                    let name = field.name_ref().or_else(|| match &expr {
                        Some(ast::Expr::PathExpr(expr)) => expr.path()?.segment()?.name_ref(),
                        _ => None,
                    });
                    !name.is_some_and(|name| matches!(name.text(), "kind" | "source"))
                        || expr.is_some_and(|expr| syntax::origin(&expr, parameters))
                })
        }
        Some(ast::Expr::CallExpr(call)) => {
            matches!(call.expr(), Some(ast::Expr::PathExpr(expr)) if expr.path().map(syntax::names).is_some_and(|names| names.len() == 2 && names[0] == "Self"))
                && call
                    .arg_list()
                    .is_some_and(|list| list.args().all(|arg| syntax::origin(&arg, parameters)))
        }
        _ => false,
    }
}

#[instrument(level = "trace", skip(item, name, parameters))]
pub(super) fn safe_return(item: &ast::Fn, name: &str, parameters: &BTreeSet<String>) -> bool {
    if parameters.is_empty() {
        return false;
    }
    sole_tail(item).is_some_and(|expr| match expr {
        ast::Expr::CallExpr(call) => {
            matches!(call.expr(), Some(ast::Expr::PathExpr(expr)) if expr.path().map(syntax::names).is_some_and(|names| names.len() >= 2 && names[names.len() - 2] == name)) && call.arg_list().is_some_and(|list| list.args().all(|arg| syntax::origin(&arg, parameters)))
        }
        _ => false,
    }) || forwarding_result(item, parameters)
}

#[instrument(level = "trace", skip(item, parameters))]
fn forwarding_result(item: &ast::Fn, parameters: &BTreeSet<String>) -> bool {
    let Some(body) = item.body().and_then(|body| body.stmt_list()) else {
        return false;
    };
    let statements: Vec<_> = body.statements().collect();
    let [ast::Stmt::LetStmt(local), ast::Stmt::ExprStmt(stmt)] = statements.as_slice() else {
        return false;
    };
    let Some(ast::Pat::IdentPat(binding)) = local.pat() else {
        return false;
    };
    let Some(binding) = binding.name() else {
        return false;
    };
    if !matches!(local.ty(), Some(ast::Type::PathType(ty)) if ty.path().and_then(|path| path.segment()).and_then(|part| part.name_ref()).is_some_and(|name| name.text() == "Result"))
    {
        return false;
    }
    let Some(ast::Expr::TryExpr(expr)) = stmt.expr() else {
        return false;
    };
    if stmt.semicolon_token().is_none()
        || !expr
            .expr()
            .is_some_and(|expr| syntax::bare_path(&expr, binding.text()))
    {
        return false;
    }
    let Some(ast::Expr::CallExpr(err)) = local.initializer() else {
        return false;
    };
    if !err
        .expr()
        .is_some_and(|expr| syntax::bare_path(&expr, "Err"))
    {
        return false;
    }
    let args: Vec<_> = err
        .arg_list()
        .into_iter()
        .flat_map(|list| list.args())
        .collect();
    let [ast::Expr::TupleExpr(tuple)] = args.as_slice() else {
        return false;
    };
    if !tuple.fields().all(|arg| syntax::origin(&arg, parameters)) {
        return false;
    }
    let Some(ast::Expr::CallExpr(ok)) = body.tail_expr() else {
        return false;
    };
    let args: Vec<_> = ok
        .arg_list()
        .into_iter()
        .flat_map(|list| list.args())
        .collect();
    ok.expr().is_some_and(|expr| syntax::bare_path(&expr, "Ok"))
        && matches!(args.as_slice(), [ast::Expr::TupleExpr(unit)] if unit.fields().next().is_none())
}
