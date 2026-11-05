//! Error-site projection from the loader-issued rust-analyzer tree.

use crate::CordialResult;
use crate::etiquettes::error_sites::{ErrorSiteKind, ErrorSiteRecord};
use crate::loader::{DatabaseSource, module_path_from_src_file};
use ra_ap_syntax::{
    AstNode, WalkEvent,
    ast::{self, HasArgList, HasAttrs, HasName},
};
use std::path::Path;

#[tracing::instrument(level = "debug", skip(file), err(level = "warn"))]
#[hotpath::measure]
pub(super) fn collect(
    file: &DatabaseSource<'_>,
    tree: &Path,
    root: &Path,
) -> CordialResult<Vec<ErrorSiteRecord>> {
    let mut records = Vec::new();
    let prefix = module_path_from_src_file(tree, file.path());
    let mut walk = file.tree().syntax().preorder();
    while let Some(event) = walk.next() {
        let WalkEvent::Enter(node) = event else {
            continue;
        };
        if excluded_module(&node) {
            walk.skip_subtree();
            continue;
        }
        if let Some(expression) = ast::Expr::cast(node)
            && let Some((kind, source, site, line)) = site(&expression, file)
        {
            records.push(
                ErrorSiteRecord::builder()
                    .kind(kind)
                    .context(context(expression.syntax(), &prefix))
                    .file(
                        file.path()
                            .strip_prefix(root)
                            .unwrap_or(file.path())
                            .to_path_buf(),
                    )
                    .line(line)
                    .source_snippet(short(&source))
                    .site_snippet(truncate(&site, 96))
                    .build()?,
            );
        }
    }
    Ok(records)
}

#[tracing::instrument(level = "trace", skip(node))]
pub(crate) fn excluded_module(node: &ra_ap_syntax::SyntaxNode) -> bool {
    ast::Module::cast(node.clone()).is_some_and(|module| {
        module.attrs().any(|attr| match attr.meta() {
            Some(ast::Meta::CfgMeta(meta)) => meta.cfg_predicate().is_some_and(|predicate| {
                predicate
                    .syntax()
                    .descendants_with_tokens()
                    .filter_map(|element| element.into_token())
                    .filter(|token| !token.kind().is_trivia())
                    .map(|token| token.text().to_owned())
                    .collect::<String>()
                    == "test"
            }),
            _ => false,
        })
    })
}

#[tracing::instrument(level = "trace", skip(expression, file))]
fn site(
    expression: &ast::Expr,
    file: &DatabaseSource<'_>,
) -> Option<(ErrorSiteKind, ast::Expr, String, u32)> {
    let line = file.line(expression.syntax().text_range().start());
    match expression {
        ast::Expr::TryExpr(expr) => {
            let source = expr.expr()?;
            let site = format!("{}?", short(&source));
            Some((ErrorSiteKind::QuestionMark, source, site, line))
        }
        ast::Expr::MethodCallExpr(expr) => {
            let name = expr.name_ref()?;
            let kind = match name.text() {
                "map_err" => ErrorSiteKind::MapErr,
                "ok_or" | "ok_or_else" => ErrorSiteKind::OkOr,
                _ => return None,
            };
            let source = expr.receiver()?;
            let site = format!("{}.{}(…)", short(&source), name.text());
            Some((kind, source, site, line))
        }
        ast::Expr::ReturnExpr(expr) => {
            let ast::Expr::CallExpr(call) = expr.expr()? else {
                return None;
            };
            let ast::Expr::PathExpr(path) = call.expr()? else {
                return None;
            };
            if !is_err(&path.path()?) {
                return None;
            }
            Some((
                ErrorSiteKind::ReturnErr,
                call.arg_list()?.args().next()?,
                "return Err(…)".into(),
                line,
            ))
        }
        ast::Expr::IfExpr(expr) => {
            let ast::Expr::LetExpr(condition) = expr.condition()? else {
                return None;
            };
            if !err_pattern(&condition.pat()?) {
                return None;
            }
            let source = condition.expr()?;
            let site = format!("if let Err(…) = {}", short(&source));
            Some((
                ErrorSiteKind::IfLetErr,
                source,
                site,
                file.line(condition.syntax().text_range().start()),
            ))
        }
        ast::Expr::MatchExpr(expr) => {
            if !expr
                .match_arm_list()?
                .arms()
                .filter_map(|arm| arm.pat())
                .any(|pat| err_pattern(&pat))
            {
                return None;
            }
            let source = expr.expr()?;
            let site = format!("match {} {{ Err(…) => … }}", short(&source));
            Some((ErrorSiteKind::MatchErr, source, site, line))
        }
        _ => None,
    }
}

#[tracing::instrument(level = "trace", skip(pat))]
fn err_pattern(pat: &ast::Pat) -> bool {
    match pat {
        ast::Pat::TupleStructPat(pat) => pat.path().is_some_and(|path| is_err(&path)),
        ast::Pat::PathPat(pat) => pat.path().is_some_and(|path| is_err(&path)),
        _ => false,
    }
}

#[tracing::instrument(level = "trace", skip(path))]
fn is_err(path: &ast::Path) -> bool {
    use ast::HasGenericArgs;
    path.qualifier().is_none()
        && path.segment().is_some_and(|segment| {
            segment.coloncolon_token().is_none()
                && segment.generic_arg_list().is_none()
                && segment.name_ref().is_some_and(|name| name.text() == "Err")
        })
}

#[tracing::instrument(level = "trace", skip(node, prefix))]
pub(in crate::ir) fn context(node: &ra_ap_syntax::SyntaxNode, prefix: &[String]) -> String {
    let ancestors: Vec<_> = node.ancestors().collect();
    let mut parts = prefix.to_vec();
    parts.extend(
        ancestors
            .iter()
            .rev()
            .filter_map(|node| ast::Module::cast(node.clone()))
            .filter_map(|module| module.name())
            .map(|name| name.text().to_string()),
    );
    if let Some(implementation) = ancestors
        .iter()
        .find_map(|node| ast::Impl::cast(node.clone()))
    {
        parts.push(
            implementation
                .self_ty()
                .map(|ty| impl_type(&ty))
                .unwrap_or_else(|| "?".into()),
        );
    }
    parts.extend(
        ancestors
            .iter()
            .rev()
            .filter_map(|node| ast::Fn::cast(node.clone()))
            .filter(|function| {
                !function
                    .syntax()
                    .parent()
                    .and_then(|parent| parent.parent())
                    .is_some_and(|parent| ast::Trait::can_cast(parent.kind()))
            })
            .filter_map(|function| function.name())
            .map(|name| name.text().to_string()),
    );
    if parts.is_empty() {
        "<crate>".into()
    } else {
        parts.join("::")
    }
}

#[tracing::instrument(level = "trace", skip(ty))]
fn impl_type(ty: &ast::Type) -> String {
    match ty {
        ast::Type::PathType(ty) => ty
            .path()
            .and_then(|path| path.segment())
            .and_then(|segment| segment.name_ref())
            .map(|name| name.text().to_string())
            .unwrap_or_else(|| "?".into()),
        ast::Type::RefType(ty) => ty
            .ty()
            .map(|ty| impl_type(&ty))
            .unwrap_or_else(|| "?".into()),
        ast::Type::ParenType(ty) => ty
            .ty()
            .map(|ty| impl_type(&ty))
            .unwrap_or_else(|| "?".into()),
        _ => "?".into(),
    }
}

#[tracing::instrument(level = "trace", skip(expression))]
fn short(expression: &ast::Expr) -> String {
    truncate(&snippet(expression), 72)
}

#[tracing::instrument(level = "trace", skip(expression))]
pub(in crate::ir) fn snippet(expression: &ast::Expr) -> String {
    match expression {
        ast::Expr::MacroExpr(expr) => expr
            .macro_call()
            .and_then(|call| call.path())
            .and_then(|path| path.segment())
            .and_then(|segment| segment.name_ref())
            .map(|name| format!("{}!(…)", name.text()))
            .unwrap_or_else(|| "…".into()),
        ast::Expr::CallExpr(expr) => format!(
            "{}(…)",
            expr.expr()
                .map(|expr| callee(&expr))
                .unwrap_or_else(|| "…".into())
        ),
        ast::Expr::MethodCallExpr(expr) => format!(
            "{}.{}(…)",
            expr.receiver()
                .map(|expr| snippet(&expr))
                .unwrap_or_else(|| "…".into()),
            expr.name_ref()
                .map(|name| name.text().to_string())
                .unwrap_or_else(|| "…".into())
        ),
        ast::Expr::PathExpr(expr) => expr
            .path()
            .map(|path| path_label(&path))
            .unwrap_or_else(|| "…".into()),
        ast::Expr::FieldExpr(expr) => format!(
            "{}.{}",
            expr.expr()
                .map(|expr| snippet(&expr))
                .unwrap_or_else(|| "…".into()),
            expr.name_ref()
                .map(|name| name.text().to_string())
                .unwrap_or_else(|| "…".into())
        ),
        ast::Expr::TryExpr(expr) => format!(
            "{}?",
            expr.expr()
                .map(|expr| snippet(&expr))
                .unwrap_or_else(|| "…".into())
        ),
        _ => "…".into(),
    }
}

#[tracing::instrument(level = "trace", skip(expr))]
fn callee(expr: &ast::Expr) -> String {
    match expr {
        ast::Expr::PathExpr(_) | ast::Expr::FieldExpr(_) => snippet(expr),
        _ => "…".into(),
    }
}

#[tracing::instrument(level = "trace", skip(path))]
pub(crate) fn path_label(path: &ast::Path) -> String {
    let mut names = Vec::new();
    let mut current = Some(path.clone());
    while let Some(path) = current {
        if let Some(segment) = path.segment() {
            if let Some(name) = segment.name_ref() {
                names.push(name.text().to_string());
            } else if let Some(anchor) = segment.type_anchor()
                && anchor.as_token().is_some()
                && let Some(trait_path) = anchor
                    .syntax()
                    .children()
                    .filter_map(ast::PathType::cast)
                    .last()
                    .and_then(|ty| ty.path())
            {
                names.push(path_label(&trait_path));
            }
        }
        current = path.qualifier();
    }
    names.reverse();
    names.join("::")
}

#[tracing::instrument(level = "trace")]
pub(in crate::ir) fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.into()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}
