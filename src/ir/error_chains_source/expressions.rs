//! Existing chain-preservation decisions over canonical expression nodes.

use super::{Fact, types::type_label};
use crate::etiquettes::{error_chain::ErrorChainProbeId, error_sites::infer_foreign_error_type};
use crate::ir::error_sites_source::native::{path_label, snippet, truncate};
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasArgList},
};

#[tracing::instrument(level = "trace", skip(node))]
pub(super) fn collect(node: &SyntaxNode) -> Option<Fact> {
    if !umbrella_return(node) {
        return None;
    }
    let (rule, source, suffix) = match ast::Expr::cast(node.clone())? {
        ast::Expr::TryExpr(expr) => {
            let source = expr.expr()?;
            if matches!(&source, ast::Expr::MethodCallExpr(call) if call.name_ref().is_some_and(|name| name.text() == "ok"))
                || contains_map_err(&source)
            {
                return None;
            }
            (ErrorChainProbeId::PreservedQuestionMark001, source, "?")
        }
        ast::Expr::MethodCallExpr(call) if call.name_ref()?.text() == "map_err" => {
            let converter = call.arg_list()?.args().next()?;
            if contains_to_string(&converter) || !preserves(&converter) {
                return None;
            }
            (
                ErrorChainProbeId::PreservedMapErr001,
                call.receiver()?,
                ".map_err(…)",
            )
        }
        _ => return None,
    };
    let text = truncate(&snippet(&source), 96);
    let (foreign, _, _) = infer_foreign_error_type(&text)?;
    Some(Fact {
        rule,
        node: node.clone(),
        snippet: format!("{text}{suffix}"),
        foreign: Some(foreign),
    })
}

#[tracing::instrument(level = "trace", skip(node))]
fn umbrella_return(node: &SyntaxNode) -> bool {
    let function = node.ancestors().filter_map(ast::Fn::cast).find(|function| {
        !function
            .syntax()
            .parent()
            .and_then(|parent| parent.parent())
            .is_some_and(|parent| ast::Trait::can_cast(parent.kind()))
    });
    let Some(label) = function
        .and_then(|function| function.ret_type()?.ty())
        .map(|ty| type_label(&ty))
    else {
        return false;
    };
    !label.starts_with("Option")
        && !label.contains("Option<")
        && !label.contains("io::Result")
        && ![
            "std::io::Error",
            "serde_json::Error",
            "syn::Error",
            "csv::Error",
            "cargo_metadata::Error",
        ]
        .iter()
        .any(|foreign| label.contains(foreign))
        && label.contains("Result")
}

#[tracing::instrument(level = "trace", skip(expr))]
fn path_name(expr: &ast::Expr) -> Option<String> {
    let ast::Expr::PathExpr(expr) = expr else {
        return None;
    };
    expr.path().map(|path| path_label(&path))
}

#[tracing::instrument(level = "trace", skip(expr))]
fn is_from(expr: &ast::Expr) -> bool {
    path_name(expr).is_some_and(|name| name.rsplit("::").next() == Some("from"))
}

#[tracing::instrument(level = "trace", skip(expr))]
fn into_or_from(expr: &ast::Expr) -> bool {
    path_name(expr)
        .is_some_and(|name| name == "Into::into" || name.rsplit("::").next() == Some("from"))
}

#[tracing::instrument(level = "trace", skip(expr))]
fn drops_source(expr: &ast::Expr) -> bool {
    path_name(expr).is_some_and(|name| {
        matches!(
            name.rsplit("::").next(),
            Some("invariant" | "to_string" | "from_str")
        )
    })
}

#[tracing::instrument(level = "trace", skip(expr))]
fn preserves(expr: &ast::Expr) -> bool {
    match expr {
        ast::Expr::PathExpr(_) => into_or_from(expr) || !drops_source(expr),
        ast::Expr::CallExpr(call) => call.expr().is_some_and(|expr| is_from(&expr)),
        ast::Expr::ClosureExpr(closure) => closure.body().is_some_and(|body| {
            !contains_to_string(&body) && (contains_source_field(&body) || forwards_binding(&body))
        }),
        _ => false,
    }
}

#[tracing::instrument(level = "trace", skip(expr))]
fn error_binding(expr: &ast::Expr) -> bool {
    match expr {
        ast::Expr::PathExpr(expr) => expr.path().is_some_and(|path| {
            use ast::HasGenericArgs;
            path.qualifier().is_none()
                && path.segment().is_some_and(|segment| {
                    segment.coloncolon_token().is_none()
                        && segment.generic_arg_list().is_none()
                        && segment
                            .name_ref()
                            .is_some_and(|name| matches!(name.text(), "e" | "err" | "error"))
                })
        }),
        ast::Expr::ParenExpr(paren) => paren.expr().is_some_and(|expr| error_binding(&expr)),
        _ => false,
    }
}

#[tracing::instrument(level = "trace", skip(expr))]
fn forwards_binding(expr: &ast::Expr) -> bool {
    match expr {
        ast::Expr::CallExpr(call) => call.expr().is_some_and(|callee| {
            is_from(&callee)
                || !drops_source(&callee)
                    && call
                        .arg_list()
                        .is_some_and(|args| args.args().any(|arg| error_binding(&arg)))
        }),
        ast::Expr::PathExpr(_) => into_or_from(expr),
        ast::Expr::BlockExpr(block) => block
            .stmt_list()
            .and_then(|list| {
                list.tail_expr().or_else(|| {
                    list.statements().last().and_then(|stmt| match stmt {
                        ast::Stmt::ExprStmt(stmt) => stmt.expr(),
                        _ => None,
                    })
                })
            })
            .is_some_and(|expr| forwards_binding(&expr)),
        ast::Expr::ParenExpr(paren) => paren.expr().is_some_and(|expr| forwards_binding(&expr)),
        _ => false,
    }
}

#[tracing::instrument(level = "trace", skip(expr))]
fn contains_to_string(expr: &ast::Expr) -> bool {
    match expr {
        ast::Expr::MethodCallExpr(call)
            if call
                .name_ref()
                .is_some_and(|name| name.text() == "to_string") =>
        {
            call.receiver().is_some_and(|expr| error_binding(&expr))
        }
        ast::Expr::CallExpr(_)
        | ast::Expr::MethodCallExpr(_)
        | ast::Expr::FieldExpr(_)
        | ast::Expr::ParenExpr(_) => children(expr).iter().any(contains_to_string),
        ast::Expr::ClosureExpr(closure) => {
            closure.body().is_some_and(|expr| contains_to_string(&expr))
        }
        ast::Expr::RecordExpr(record) => record.record_expr_field_list().is_some_and(|fields| {
            fields
                .fields()
                .filter_map(|field| field.expr())
                .any(|expr| contains_to_string(&expr))
        }),
        ast::Expr::BlockExpr(block) => block.stmt_list().is_some_and(|list| {
            list.statements()
                .filter_map(|stmt| match stmt {
                    ast::Stmt::ExprStmt(stmt) => stmt.expr(),
                    _ => None,
                })
                .chain(list.tail_expr())
                .any(|expr| contains_to_string(&expr))
        }),
        _ => false,
    }
}

#[tracing::instrument(level = "trace", skip(expr))]
fn contains_source_field(expr: &ast::Expr) -> bool {
    match expr {
        ast::Expr::RecordExpr(record) => record.record_expr_field_list().is_some_and(|fields| {
            fields.fields().any(|field| {
                field
                    .name_ref()
                    .map(|name| name.text().to_string())
                    .or_else(|| field.expr().as_ref().and_then(path_name))
                    .is_some_and(|name| matches!(name.as_str(), "source" | "err"))
            })
        }),
        ast::Expr::CallExpr(_)
        | ast::Expr::MethodCallExpr(_)
        | ast::Expr::FieldExpr(_)
        | ast::Expr::ParenExpr(_) => children(expr).iter().any(contains_source_field),
        ast::Expr::ClosureExpr(closure) => closure
            .body()
            .is_some_and(|expr| contains_source_field(&expr)),
        _ => false,
    }
}

#[tracing::instrument(level = "trace", skip(expr))]
fn contains_map_err(expr: &ast::Expr) -> bool {
    match expr {
        ast::Expr::MethodCallExpr(call)
            if call.name_ref().is_some_and(|name| name.text() == "map_err") =>
        {
            true
        }
        ast::Expr::TryExpr(expr) => expr.expr().is_some_and(|expr| contains_map_err(&expr)),
        ast::Expr::CallExpr(_)
        | ast::Expr::MethodCallExpr(_)
        | ast::Expr::FieldExpr(_)
        | ast::Expr::ParenExpr(_) => children(expr).iter().any(contains_map_err),
        _ => false,
    }
}

#[tracing::instrument(level = "trace", skip(expr))]
fn children(expr: &ast::Expr) -> Vec<ast::Expr> {
    match expr {
        ast::Expr::CallExpr(call) => call
            .expr()
            .into_iter()
            .chain(call.arg_list().into_iter().flat_map(|args| args.args()))
            .collect(),
        ast::Expr::MethodCallExpr(call) => call
            .receiver()
            .into_iter()
            .chain(call.arg_list().into_iter().flat_map(|args| args.args()))
            .collect(),
        ast::Expr::FieldExpr(field) => field.expr().into_iter().collect(),
        ast::Expr::ParenExpr(paren) => paren.expr().into_iter().collect(),
        _ => Vec::new(),
    }
}
