//! Conversion sites and the limited expression shapes understood by assessment.

use super::{
    super::{
        Conversion, SignatureChainIndex,
        facts::{Expr, Type},
    },
    shapes,
};
use crate::loader::DatabaseSource;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasName},
};
use std::collections::BTreeMap;

impl SignatureChainIndex {
    #[tracing::instrument(level = "trace", skip(self, function, file))]
    pub(super) fn native_calls(
        &mut self,
        function: &ast::Fn,
        scope: &[String],
        file: &DatabaseSource<'_>,
    ) {
        let Some(block) = function.body() else {
            return;
        };
        let params: BTreeMap<String, Type> = function
            .param_list()
            .into_iter()
            .flat_map(|list| list.params())
            .filter_map(|param| {
                let ast::Pat::IdentPat(binding) = param.pat()? else {
                    return None;
                };
                Some((binding.name()?.text().to_string(), shapes::ty(&param.ty()?)))
            })
            .collect();
        for call in block
            .syntax()
            .descendants()
            .filter_map(ast::MethodCallExpr::cast)
        {
            if !call.name_ref().is_some_and(|name| name.text() == "map_err") {
                continue;
            }
            let Some(ast::Expr::ClosureExpr(closure)) =
                call.arg_list().and_then(|list| list.args().next())
            else {
                continue;
            };
            let Some(ast::Pat::IdentPat(binding)) = closure
                .param_list()
                .and_then(|list| list.params().next())
                .and_then(|param| param.pat())
            else {
                continue;
            };
            // A typed closure parameter is a different pattern in the compatibility grammar.
            if closure
                .param_list()
                .and_then(|list| list.params().next())
                .is_some_and(|param| param.ty().is_some())
            {
                continue;
            }
            let Some(name) = binding.name() else {
                continue;
            };
            let Some(body) = closure.body() else {
                continue;
            };
            let Some(receiver) = call.receiver() else {
                continue;
            };
            let binding = name.text().to_string();
            let has_bound_call = body
                .syntax()
                .descendants()
                .filter_map(ast::CallExpr::cast)
                .any(|call| {
                    call.arg_list()
                        .is_some_and(|list| list.args().any(|arg| is_binding(&arg, &binding)))
                });
            self.conversions.push(Conversion {
                scope: scope.to_vec(),
                receiver: expression(&receiver),
                body: expression(&body),
                has_bound_call,
                binding,
                params: params.clone(),
                file: file.path().to_path_buf(),
                line: file.line(call.syntax().text_range().start()),
            });
        }
    }
}

#[tracing::instrument(level = "trace", skip(value))]
pub(super) fn expression(value: &ast::Expr) -> Expr {
    match value {
        ast::Expr::PathExpr(value) => value
            .path()
            .map(|path| Expr::Path(shapes::path(&path)))
            .unwrap_or(Expr::Unsupported),
        ast::Expr::CallExpr(value) => Expr::Call {
            path: match value.expr() {
                Some(ast::Expr::PathExpr(value)) => value.path().map(|path| shapes::path(&path)),
                _ => None,
            },
            args: value
                .arg_list()
                .into_iter()
                .flat_map(|list| list.args())
                .map(|arg| expression(&arg))
                .collect(),
        },
        ast::Expr::BlockExpr(value)
            if value.async_token().is_none()
                && value.const_token().is_none()
                && value.unsafe_token().is_none()
                && value.gen_token().is_none()
                && value.try_block_modifier().is_none() =>
        {
            last_expression(value)
                .map(|value| Expr::Transparent(Box::new(expression(&value))))
                .unwrap_or(Expr::Unsupported)
        }
        ast::Expr::ParenExpr(value) => value
            .expr()
            .map(|value| Expr::Transparent(Box::new(expression(&value))))
            .unwrap_or(Expr::Unsupported),
        _ => Expr::Unsupported,
    }
}

#[tracing::instrument(level = "trace", skip(value))]
pub(super) fn is_binding(value: &ast::Expr, binding: &str) -> bool {
    matches!(value, ast::Expr::PathExpr(value) if value.path().is_some_and(|path| shapes::path(&path).ident() == Some(binding)))
}

#[tracing::instrument(level = "trace", skip(block))]
pub(super) fn last_expression(block: &ast::BlockExpr) -> Option<ast::Expr> {
    let statements = block.stmt_list()?;
    statements
        .tail_expr()
        .or_else(|| match statements.statements().last()? {
            ast::Stmt::ExprStmt(statement) => statement.expr(),
            _ => None,
        })
}
