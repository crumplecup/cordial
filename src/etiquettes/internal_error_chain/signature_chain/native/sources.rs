//! Recognize only source implementations already proven by the lint's grammar.

use super::super::ManualSource;
use super::expressions::{is_binding, last_expression};
use super::shapes;
use ra_ap_syntax::ast::{self, HasArgList, HasName};

#[tracing::instrument(level = "trace", skip(method))]
pub(super) fn relationship(method: &ast::Fn) -> ManualSource {
    let Some(tail) = method.body().and_then(|body| last_expression(&body)) else {
        return ManualSource::Opaque;
    };
    if named_field(&tail) {
        return ManualSource::NamedField;
    }
    variants(&tail)
        .map(ManualSource::Variants)
        .unwrap_or(ManualSource::Opaque)
}

#[tracing::instrument(level = "trace", skip(expr))]
fn some_argument(expr: &ast::Expr) -> Option<ast::Expr> {
    let ast::Expr::CallExpr(call) = expr else {
        return None;
    };
    if !is_binding(&call.expr()?, "Some") {
        return None;
    }
    let args = call.arg_list()?;
    if args.args().count() != 1 {
        return None;
    }
    args.args().next()
}

#[tracing::instrument(level = "trace", skip(expr))]
fn named_field(expr: &ast::Expr) -> bool {
    let Some(ast::Expr::RefExpr(reference)) = some_argument(expr) else {
        return false;
    };
    let Some(ast::Expr::FieldExpr(field)) = reference.expr() else {
        return false;
    };
    field.name_ref().is_some_and(|name| name.text() == "source")
        && field.expr().is_some_and(|base| is_binding(&base, "self"))
}

#[tracing::instrument(level = "trace", skip(expr))]
fn variants(expr: &ast::Expr) -> Option<Vec<String>> {
    let ast::Expr::MatchExpr(matched) = expr else {
        return None;
    };
    if !is_binding(&matched.expr()?, "self") {
        return None;
    }
    let mut variants = Vec::new();
    for arm in matched.match_arm_list()?.arms() {
        let body = arm.expr()?;
        if is_binding(&body, "None") {
            continue;
        }
        let ast::Expr::PathExpr(source) = some_argument(&body)? else {
            return None;
        };
        let binding = shapes::path(&source.path()?);
        let binding = binding.ident()?;
        let variant = match arm.pat()? {
            ast::Pat::TupleStructPat(pattern) if pattern.fields().count() == 1 => {
                let ast::Pat::IdentPat(value) = pattern.fields().next()? else {
                    return None;
                };
                if value.name()?.text() != binding {
                    return None;
                }
                shapes::path(&pattern.path()?).parts().last()?.clone()
            }
            ast::Pat::RecordPat(pattern) => {
                let field = pattern.record_pat_field_list()?.fields().find(|field| {
                    matches!(field.pat(), Some(ast::Pat::IdentPat(value)) if value.name().is_some_and(|name| name.text() == binding))
                })?;
                let name = field
                    .name_ref()
                    .map(|name| name.text().to_string())
                    .or_else(|| {
                        let ast::Pat::IdentPat(value) = field.pat()? else {
                            return None;
                        };
                        value.name().map(|name| name.text().to_string())
                    })?;
                if name != "source" {
                    return None;
                }
                shapes::path(&pattern.path()?).parts().last()?.clone()
            }
            _ => return None,
        };
        variants.push(variant);
    }
    Some(variants)
}
