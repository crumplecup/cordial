//! Exact expression recipes on the canonical tree, independent of candidates.
use super::projection::{StringOrigin, StringProof};
use super::replacements::{InitializerExpressionFact, StorageForm, ValueSource};
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasArgList, HasGenericArgs},
};
use std::collections::{BTreeMap, BTreeSet};
use tracing::instrument;

#[instrument(level = "trace", skip(node))]
pub(super) fn range(node: &SyntaxNode) -> (usize, usize) {
    (
        node.text_range().start().into(),
        node.text_range().end().into(),
    )
}

#[instrument(level = "trace", skip(path))]
pub(super) fn names(path: &ast::Path) -> Vec<String> {
    let mut names = path
        .qualifier()
        .map(|path| self::names(&path))
        .unwrap_or_default();
    if let Some(name) = path.segment().and_then(|part| part.name_ref()) {
        names.push(name.text().to_string());
    }
    names
}

#[instrument(level = "trace", skip(path))]
pub(super) fn unanchored(path: &ast::Path) -> bool {
    path.segment()
        .is_some_and(|part| part.type_anchor().is_none())
        && path.qualifier().is_none_or(|path| unanchored(&path))
}

#[instrument(level = "trace", skip(path))]
pub(super) fn bare(path: &ast::Path) -> Option<String> {
    let part = path.segment()?;
    (path.qualifier().is_none()
        && path.coloncolon_token().is_none()
        && part.coloncolon_token().is_none()
        && part.type_anchor().is_none()
        && part.generic_arg_list().is_none()
        && part.parenthesized_arg_list().is_none())
    .then(|| part.name_ref().map(|name| name.text().to_string()))
    .flatten()
}

#[instrument(level = "trace", skip(expr))]
fn without_parens(mut expr: ast::Expr) -> Option<ast::Expr> {
    while let ast::Expr::ParenExpr(paren) = expr {
        expr = paren.expr()?;
    }
    Some(expr)
}

#[instrument(level = "trace", skip(call))]
pub(super) fn location_caller(call: &ast::CallExpr) -> bool {
    if call
        .arg_list()
        .is_none_or(|list| list.args().next().is_some())
    {
        return false;
    }
    let Some(ast::Expr::PathExpr(path)) = call.expr() else {
        return false;
    };
    let Some(path) = path.path() else {
        return false;
    };
    matches!(names(&path).as_slice(), [location, caller] if location == "Location" && caller == "caller")
        || matches!(names(&path).as_slice(), [root, panic, location, caller] if (root == "std" || root == "core") && panic == "panic" && location == "Location" && caller == "caller")
}

#[instrument(level = "trace", skip(expr))]
fn value(expr: ast::Expr) -> Option<(StorageForm, ast::Expr)> {
    let expr = without_parens(expr)?;
    if let ast::Expr::MethodCallExpr(method) = &expr {
        if method
            .name_ref()
            .is_some_and(|name| matches!(name.text(), "to_string" | "to_owned" | "into"))
            && method.arg_list()?.args().next().is_none()
        {
            return Some((StorageForm::Method, without_parens(method.receiver()?)?));
        }
    }
    if let ast::Expr::CallExpr(call) = &expr {
        let args: Vec<_> = call.arg_list()?.args().collect();
        if let (Some(ast::Expr::PathExpr(path)), [arg]) =
            (call.expr().and_then(without_parens), args.as_slice())
        {
            let path = path.path()?;
            let parts = names(&path);
            if unanchored(&path)
                && (parts == ["String", "from"]
                    || matches!(parts.as_slice(), [root, module, string, from] if (root == "std" || root == "alloc") && module == "string" && string == "String" && from == "from"))
            {
                return Some((StorageForm::StringFrom, without_parens(arg.clone())?));
            }
        }
    }
    Some((StorageForm::Bare, expr))
}

#[instrument(level = "trace", skip(expr, aliases))]
pub(super) fn file(
    expr: &ast::Expr,
    field: &str,
    shorthand: bool,
    aliases: &BTreeSet<String>,
) -> Option<InitializerExpressionFact> {
    let (form, value) = value(expr.clone())?;
    let ast::Expr::MethodCallExpr(method) = &value else {
        return None;
    };
    if method.name_ref()?.text() != "file" || method.arg_list()?.args().next().is_some() {
        return None;
    }
    let valid = match without_parens(method.receiver()?)? {
        ast::Expr::PathExpr(path) => path
            .path()
            .and_then(|path| bare(&path))
            .is_some_and(|name| aliases.contains(&name)),
        ast::Expr::CallExpr(call) => location_caller(&call),
        _ => false,
    };
    valid.then(|| {
        InitializerExpressionFact::new(
            range(expr.syntax()),
            range(value.syntax()),
            form,
            ValueSource::CallSiteFile,
            field.to_string(),
            shorthand,
        )
    })
}

#[instrument(level = "trace", skip(expr, bindings))]
pub(super) fn string(
    expr: &ast::Expr,
    field: &str,
    shorthand: bool,
    bindings: &BTreeMap<String, (String, usize)>,
) -> Option<StringProof> {
    let (form, value) = value(expr.clone())?;
    let mut local_binding = false;
    let origin = match &value {
        ast::Expr::Literal(lit) if matches!(lit.kind(), ast::LiteralKind::String(_)) => {
            StringOrigin::Static
        }
        ast::Expr::PathExpr(path) => {
            let path = path.path()?;
            if !unanchored(&path) {
                return None;
            }
            if let Some(name) = bare(&path) {
                let (lifetime, end) = bindings.get(&name)?;
                if *end > range(value.syntax()).0 {
                    return None;
                }
                local_binding = true;
                if lifetime == "static" {
                    StringOrigin::Static
                } else {
                    StringOrigin::Lifetime(lifetime.clone())
                }
            } else {
                let parts = names(&path);
                let [scope, name] = parts.as_slice() else {
                    return None;
                };
                if scope != "self" {
                    return None;
                }
                StringOrigin::FileConstant(name.clone())
            }
        }
        ast::Expr::MacroExpr(invocation) => {
            let path = invocation.macro_call()?.path()?;
            let root = path.qualifier()?;
            if root.qualifier().is_some()
                || (root.coloncolon_token().is_none()
                    && root.segment()?.coloncolon_token().is_none())
            {
                return None;
            }
            let parts = names(&path);
            if !matches!(parts.as_slice(), [root, name] if (root == "std" || root == "core") && (name == "concat" || name == "include_str"))
            {
                return None;
            }
            StringOrigin::Static
        }
        _ => return None,
    };
    Some(StringProof::new(
        InitializerExpressionFact::new(
            range(expr.syntax()),
            range(value.syntax()),
            form,
            ValueSource::StaticString { local_binding },
            field.to_string(),
            shorthand,
        ),
        origin,
    ))
}

#[instrument(level = "trace", skip(expr))]
pub(super) fn empty_file(expr: &ast::Expr) -> Option<InitializerExpressionFact> {
    let form = match expr {
        ast::Expr::Literal(lit) if matches!(lit.kind(), ast::LiteralKind::String(value) if value.value().is_ok_and(|value| value.is_empty())) => {
            StorageForm::Bare
        }
        ast::Expr::CallExpr(call) if call.arg_list()?.args().next().is_none() => {
            let ast::Expr::PathExpr(path) = call.expr()? else {
                return None;
            };
            let path = path.path()?;
            if !unanchored(&path)
                || names(&path) != ["String", "new"]
                || path
                    .syntax()
                    .descendants()
                    .filter_map(ast::PathSegment::cast)
                    .any(|part| {
                        part.generic_arg_list().is_some() || part.parenthesized_arg_list().is_some()
                    })
            {
                return None;
            }
            StorageForm::EmptyString
        }
        _ => return None,
    };
    let whole = range(expr.syntax());
    Some(InitializerExpressionFact::new(
        whole,
        whole,
        form,
        ValueSource::EmptyFile,
        "file".to_string(),
        false,
    ))
}
