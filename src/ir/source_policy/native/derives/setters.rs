//! Assignment and owned-input shapes for setter recommendations.
use super::exprs;
use crate::ir::DeriveSetterShape as Shape;
use ra_ap_syntax::ast::{self, HasArgList};

#[tracing::instrument(level = "trace", skip(stmts, params))]
pub(super) fn classify(
    stmts: &[Option<ast::Expr>],
    field: &str,
    params: &[String],
) -> Option<Shape> {
    if params.len() != 1 {
        return None;
    }
    let expr = match stmts {
        [Some(assign)] => assign,
        [Some(assign), Some(tail)] if returns_self(tail.clone()) => assign,
        _ => return None,
    };
    let ast::Expr::BinExpr(assign) = expr else {
        return None;
    };
    if assign.op_kind() != Some(ast::BinaryOp::Assignment { op: None })
        || !exprs::self_field(assign.lhs()?, field)
    {
        return None;
    }
    rhs(assign.rhs()?, &params[0])
}

#[tracing::instrument(level = "trace", skip(expr))]
fn returns_self(expr: ast::Expr) -> bool {
    match expr {
        ast::Expr::ReturnExpr(expr) => expr.expr().is_some_and(exprs::is_self),
        other => exprs::is_self(other),
    }
}

#[tracing::instrument(level = "trace", skip(expr))]
fn rhs(expr: ast::Expr, param: &str) -> Option<Shape> {
    match expr {
        ast::Expr::ParenExpr(expr) => rhs(expr.expr()?, param),
        ast::Expr::CallExpr(call) if some_path(call.expr()?) => {
            let args: Vec<_> = call.arg_list()?.args().collect();
            let [arg] = args.as_slice() else {
                return None;
            };
            Some(if owned_input(arg.clone(), param)? {
                Shape::StripOptionInto
            } else {
                Shape::StripOption
            })
        }
        other => Some(if owned_input(other, param)? {
            Shape::Into
        } else {
            Shape::Assign
        }),
    }
}

#[tracing::instrument(level = "trace", skip(expr))]
fn some_path(expr: ast::Expr) -> bool {
    matches!(expr, ast::Expr::PathExpr(expr) if expr.path().and_then(|path| path.segment()).and_then(|part| part.name_ref()).is_some_and(|name| name.text() == "Some"))
}

/// Whether a supported ownership conversion wraps the parameter.
#[tracing::instrument(level = "trace", skip(expr))]
fn owned_input(expr: ast::Expr, param: &str) -> Option<bool> {
    match expr {
        ast::Expr::ParenExpr(expr) => owned_input(expr.expr()?, param),
        ast::Expr::MethodCallExpr(call)
            if exprs::no_arguments(&call)
                && matches!(
                    call.name_ref()?.text(),
                    "into" | "clone" | "to_owned" | "to_string"
                ) =>
        {
            owned_input(call.receiver()?, param).map(|_| true)
        }
        other => (exprs::bare_path(&other).as_deref() == Some(param)).then_some(false),
    }
}
