//! Retention and stringification of caught bindings in canonical syntax.

use super::expressions::{binding, last_expr, macro_parts};
use ra_ap_syntax::{
    AstNode, WalkEvent,
    ast::{self, HasArgList, HasName},
};

#[tracing::instrument(level = "trace", skip(expr))]
pub(super) fn map_err_discards(expr: &ast::Expr) -> bool {
    let ast::Expr::ClosureExpr(closure) = expr else {
        return false;
    };
    let Some(ast::Pat::IdentPat(pat)) = closure
        .param_list()
        .and_then(|params| params.params().next())
        .and_then(|param| param.pat())
    else {
        return false;
    };
    let Some(name) = pat.name() else {
        return false;
    };
    closure
        .body()
        .is_some_and(|body| discards(&body, name.text()))
}

#[tracing::instrument(level = "trace", skip(expr))]
pub(super) fn discards(expr: &ast::Expr, name: &str) -> bool {
    let (stringified, _) = uses(expr, name);
    stringified && result(expr).is_none_or(|expr| !uses(&expr, name).1)
}

#[tracing::instrument(level = "trace", skip(expr))]
fn result(expr: &ast::Expr) -> Option<ast::Expr> {
    match expr {
        ast::Expr::BlockExpr(block) => last_expr(block).as_ref().and_then(result),
        ast::Expr::ParenExpr(expr) => expr.expr().as_ref().and_then(result),
        ast::Expr::ReturnExpr(expr) => expr.expr().as_ref().and_then(result),
        _ => Some(expr.clone()),
    }
}

#[tracing::instrument(level = "trace", skip(expr))]
fn uses(expr: &ast::Expr, name: &str) -> (bool, bool) {
    let mut stringified = false;
    let mut retained = false;
    let mut walk = expr.syntax().preorder();
    while let Some(event) = walk.next() {
        let WalkEvent::Enter(node) = event else {
            continue;
        };
        match ast::Expr::cast(node) {
            Some(ast::Expr::MethodCallExpr(call))
                if call
                    .name_ref()
                    .is_some_and(|name| name.text() == "to_string")
                    && call.receiver().is_some_and(|expr| binding(&expr, name)) =>
            {
                stringified = true;
                walk.skip_subtree();
            }
            Some(ast::Expr::MacroExpr(expr)) => {
                let (macro_name, tokens) = macro_parts(&expr);
                let compact = tokens.replace(' ', "");
                if matches!(
                    macro_name.as_str(),
                    "format" | "format_args" | "write" | "writeln"
                ) && [
                    format!("{{{name}}}"),
                    format!("{{{name}:"),
                    format!(",{name}"),
                    format!("({name})"),
                ]
                .iter()
                .any(|capture| compact.contains(capture))
                {
                    stringified = true;
                }
                walk.skip_subtree();
            }
            Some(ast::Expr::CallExpr(call)) => {
                retained |= call
                    .arg_list()
                    .is_some_and(|args| args.args().any(|arg| binding(&arg, name)));
            }
            Some(ast::Expr::RecordExpr(record)) => {
                retained |= record.record_expr_field_list().is_some_and(|fields| {
                    fields.fields().any(|field| {
                        (field.name_ref().is_some_and(|name| name.text() == "source")
                            || field.colon_token().is_none()
                                && field.expr().is_some_and(|expr| binding(&expr, "source")))
                            && field
                                .expr()
                                .map(|expr| binding(&expr, name))
                                .unwrap_or(name == "source")
                    })
                });
            }
            _ => {}
        }
    }
    (stringified, retained)
}
