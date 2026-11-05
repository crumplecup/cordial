//! Lifetime origins from written signatures and constructor expressions.
use super::paths;
use ra_ap_syntax::{
    AstNode, SyntaxKind,
    ast::{self, HasName},
};
use std::collections::{BTreeMap, BTreeSet};
use tracing::instrument;

#[instrument(level = "debug", skip(function))]
pub(super) fn parameters(function: &ast::Fn) -> BTreeMap<String, BTreeSet<String>> {
    let body = function.body().map(|body| body.syntax().text_range());
    let lifetimes: BTreeSet<_> = function
        .syntax()
        .descendants()
        .filter(|node| {
            !body.is_some_and(|body| body.contains_range(node.text_range()))
                && !node
                    .ancestors()
                    .any(|node| matches!(node.kind(), SyntaxKind::ATTR | SyntaxKind::TOKEN_TREE))
        })
        .filter_map(ast::Lifetime::cast)
        .map(|lifetime| {
            lifetime
                .syntax()
                .text()
                .to_string()
                .trim_start_matches('\'')
                .to_string()
        })
        .collect();
    lifetimes
        .into_iter()
        .map(|lifetime| {
            let mut names = BTreeSet::new();
            for param in function
                .param_list()
                .into_iter()
                .flat_map(|list| list.params())
            {
                if let (Some(pat), Some(ty)) = (param.pat(), param.ty()) {
                    collect_parameter(pat, ty, &lifetime, &mut names);
                }
            }
            (lifetime, names)
        })
        .collect()
}

#[instrument(level = "trace", skip(pat, ty, lifetime, names))]
fn collect_parameter(pat: ast::Pat, ty: ast::Type, lifetime: &str, names: &mut BTreeSet<String>) {
    match (pat, ty) {
        (ast::Pat::IdentPat(pat), ty)
            if ty
                .syntax()
                .descendants()
                .filter_map(ast::Lifetime::cast)
                .any(|value| {
                    value.syntax().text().to_string().trim_start_matches('\'') == lifetime
                }) =>
        {
            if let Some(name) = pat.name() {
                names.insert(name.text().to_string());
            }
        }
        (ast::Pat::TuplePat(pat), ast::Type::TupleType(ty))
            if pat.fields().count() == ty.fields().count() =>
        {
            for (pat, ty) in pat.fields().zip(ty.fields()) {
                collect_parameter(pat, ty, lifetime, names);
            }
        }
        _ => {}
    }
}

#[instrument(level = "trace", skip(expr, parameters))]
pub(super) fn origin(expr: &ast::Expr, parameters: &BTreeSet<String>) -> bool {
    match expr {
        ast::Expr::PathExpr(expr) => expr.path().is_some_and(|path| {
            let names = paths::names(path);
            names.len() == 1 && parameters.contains(&names[0])
        }),
        ast::Expr::RefExpr(reference) => reference
            .expr()
            .is_some_and(|expr| origin(&expr, parameters)),
        ast::Expr::MethodCallExpr(call)
            if ["clone", "to_owned", "to_string"]
                .iter()
                .any(|name| paths::method(call, name)) =>
        {
            call.receiver()
                .is_some_and(|expr| origin(&expr, parameters))
        }
        ast::Expr::CallExpr(call) if paths::boxed_call(call, true) => {
            use ast::HasArgList;
            call.arg_list()
                .and_then(|list| list.args().next())
                .is_some_and(|expr| origin(&expr, parameters))
        }
        _ => false,
    }
}
