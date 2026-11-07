//! Expression shapes used by the existing compliance policy.

use crate::ir::error_sites_source::native::path_label;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasGenericArgs, HasName},
};

#[tracing::instrument(level = "trace", skip(expr))]
pub(super) fn path(expr: &ast::Expr) -> Option<String> {
    let ast::Expr::PathExpr(expr) = expr else {
        return None;
    };
    expr.path().map(|path| path_label(&path))
}

#[tracing::instrument(level = "trace", skip(expr))]
pub(super) fn binding(expr: &ast::Expr, name: &str) -> bool {
    let ast::Expr::PathExpr(expr) = expr else {
        return false;
    };
    expr.path().is_some_and(|path| {
        path.qualifier().is_none()
            && path.segment().is_some_and(|segment| {
                segment.coloncolon_token().is_none()
                    && segment.generic_arg_list().is_none()
                    && segment.name_ref().is_some_and(|ident| ident.text() == name)
            })
    })
}

#[tracing::instrument(level = "trace", skip(pat))]
pub(super) fn err_binding(pat: &ast::Pat) -> Option<Option<String>> {
    let (path, first) = match pat {
        ast::Pat::TupleStructPat(pat) => (pat.path()?, pat.fields().next()),
        ast::Pat::PathPat(pat) => (pat.path()?, None),
        _ => return None,
    };
    if path.qualifier().is_some()
        || path_label(&path) != "Err"
        || path.segment().is_some_and(|segment| {
            segment.coloncolon_token().is_some() || segment.generic_arg_list().is_some()
        })
    {
        return None;
    }
    Some(first.and_then(|pat| match pat {
        ast::Pat::IdentPat(ident) => ident.name().map(|name| name.text().to_string()),
        _ => None,
    }))
}

#[tracing::instrument(level = "trace", skip(expr))]
pub(super) fn err_payload(expr: &ast::Expr) -> Option<ast::Expr> {
    match expr {
        ast::Expr::ReturnExpr(ret) => ret.expr().and_then(|expr| err_payload(&expr)),
        ast::Expr::CallExpr(call)
            if call
                .expr()
                .as_ref()
                .and_then(path)
                .is_some_and(|path| path.rsplit("::").next() == Some("Err")) =>
        {
            call.arg_list()?.args().next()
        }
        _ => None,
    }
}

#[tracing::instrument(level = "trace", skip(block))]
pub(super) fn block_exprs(block: &ast::BlockExpr) -> Vec<ast::Expr> {
    block
        .stmt_list()
        .map(|list| {
            list.statements()
                .filter_map(|stmt| match stmt {
                    ast::Stmt::ExprStmt(stmt) => stmt.expr(),
                    _ => None,
                })
                .chain(list.tail_expr())
                .collect()
        })
        .unwrap_or_default()
}

#[tracing::instrument(level = "trace", skip(block))]
pub(super) fn last_expr(block: &ast::BlockExpr) -> Option<ast::Expr> {
    let list = block.stmt_list()?;
    list.tail_expr()
        .or_else(|| match list.statements().last()? {
            ast::Stmt::ExprStmt(stmt) => stmt.expr(),
            _ => None,
        })
}

#[tracing::instrument(level = "trace", skip(expr))]
pub(super) fn internal_constructor(expr: &ast::Expr) -> Option<String> {
    match expr {
        ast::Expr::CallExpr(call) => call
            .expr()
            .as_ref()
            .and_then(path)
            .filter(|path| path.rsplit("::").next() == Some("invariant")),
        ast::Expr::ClosureExpr(expr) => expr.body().as_ref().and_then(internal_constructor),
        ast::Expr::BlockExpr(expr) => block_exprs(expr).iter().find_map(internal_constructor),
        ast::Expr::ParenExpr(expr) => expr.expr().as_ref().and_then(internal_constructor),
        _ => None,
    }
}

#[tracing::instrument(level = "trace", skip(expr))]
pub(super) fn builds_syn_error(expr: &ast::Expr) -> bool {
    match expr {
        ast::Expr::CallExpr(call) => call.expr().as_ref().and_then(path).is_some_and(|path| {
            let parts: Vec<_> = path.split("::").collect();
            matches!(
                parts.as_slice(),
                [.., "Error", "new_spanned" | "new2"] | [.., "syn", "Error", "new"]
            )
        }),
        ast::Expr::ClosureExpr(expr) => expr.body().as_ref().is_some_and(builds_syn_error),
        ast::Expr::BlockExpr(expr) => last_expr(expr).as_ref().is_some_and(builds_syn_error),
        ast::Expr::ParenExpr(expr) => expr.expr().as_ref().is_some_and(builds_syn_error),
        _ => false,
    }
}

#[tracing::instrument(level = "trace", skip(expr))]
pub(super) fn macro_parts(expr: &ast::MacroExpr) -> (String, String) {
    let call = expr.macro_call();
    let name = call
        .as_ref()
        .and_then(|call| call.path())
        .map(|path| path_label(&path))
        .and_then(|path| path.rsplit("::").next().map(str::to_owned))
        .unwrap_or_default();
    // Tokens remain opaque: do not expand or parse the macro body.
    let tokens = call
        .and_then(|call| call.token_tree())
        .map(|tree| {
            let range = tree.syntax().text_range();
            tree.syntax()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .filter(|token| {
                    !token.kind().is_trivia()
                        && token.text_range().start() != range.start()
                        && token.text_range().end() != range.end()
                })
                .map(|token| token.text().to_owned())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    (name, tokens)
}

#[tracing::instrument(level = "trace", skip(expr))]
pub(super) fn uses_error(expr: &ast::Expr) -> bool {
    match expr {
        ast::Expr::PathExpr(_) => ["e", "err", "error"].iter().any(|name| binding(expr, name)),
        ast::Expr::MethodCallExpr(call)
            if call
                .name_ref()
                .is_some_and(|name| name.text() == "to_string") =>
        {
            call.receiver().as_ref().is_some_and(uses_error)
        }
        ast::Expr::MacroExpr(expr) => {
            let (_, tokens) = macro_parts(expr);
            ["{e}", "{err}", "{error}", "to_string"]
                .iter()
                .any(|text| tokens.contains(text))
        }
        ast::Expr::CallExpr(call) => call
            .arg_list()
            .is_some_and(|args| args.args().any(|arg| uses_error(&arg))),
        _ => false,
    }
}

#[tracing::instrument(level = "trace", skip(expr))]
pub(super) fn foreign_binding(expr: &ast::Expr) -> Option<String> {
    let found = match expr {
        ast::Expr::CallExpr(call) => call
            .arg_list()
            .is_some_and(|args| args.args().any(|arg| uses_error(&arg))),
        ast::Expr::MacroExpr(expr) => macro_parts(expr).1.contains("{e}"),
        _ => false,
    };
    found.then(|| "foreign-bound".into())
}

#[tracing::instrument(level = "trace", skip(expr))]
pub(super) fn contains_to_string(expr: &ast::Expr) -> bool {
    match expr {
        ast::Expr::MethodCallExpr(call)
            if call
                .name_ref()
                .is_some_and(|name| name.text() == "to_string") =>
        {
            call.receiver().as_ref().is_some_and(uses_error)
        }
        ast::Expr::CallExpr(call) => {
            call.expr().as_ref().is_some_and(contains_to_string)
                || call
                    .arg_list()
                    .is_some_and(|args| args.args().any(|arg| contains_to_string(&arg)))
        }
        ast::Expr::MethodCallExpr(call) => {
            call.receiver().as_ref().is_some_and(contains_to_string)
                || call
                    .arg_list()
                    .is_some_and(|args| args.args().any(|arg| contains_to_string(&arg)))
        }
        ast::Expr::ClosureExpr(expr) => expr.body().as_ref().is_some_and(contains_to_string),
        ast::Expr::BlockExpr(expr) => block_exprs(expr).iter().any(contains_to_string),
        ast::Expr::MacroExpr(expr) => {
            let (name, tokens) = macro_parts(expr);
            let compact = tokens.replace(' ', "");
            (matches!(
                name.as_str(),
                "format" | "format_args" | "write" | "writeln"
            ) && ["e", "err", "error"].iter().any(|name| {
                [
                    format!("{{{name}}}"),
                    format!("{{{name}:"),
                    format!(",{name}}}"),
                    format!("({name})"),
                ]
                .iter()
                .any(|text| compact.contains(text))
            })) || tokens.contains("to_string")
        }
        ast::Expr::RecordExpr(expr) => expr.record_expr_field_list().is_some_and(|fields| {
            fields
                .fields()
                .filter_map(|field| field.expr())
                .any(|expr| contains_to_string(&expr))
        }),
        ast::Expr::FieldExpr(expr) => expr.expr().as_ref().is_some_and(contains_to_string),
        ast::Expr::ParenExpr(expr) => expr.expr().as_ref().is_some_and(contains_to_string),
        _ => false,
    }
}
