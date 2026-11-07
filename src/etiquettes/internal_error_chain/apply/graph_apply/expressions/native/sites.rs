//! Located graph expression evidence, separate from traversal and scope tracking.
use super::super::{GraphExprEvent, GraphValueFact, Range, SourceArgument};
use super::{GraphExprFacts, macros, syntax};
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasArgList, HasName},
};
use std::collections::BTreeMap;
use syntax::{names, self_field};
use tracing::instrument;

#[instrument(level = "trace", skip(facts, node, owner, params, syntax))]
pub(super) fn collect(
    facts: &mut GraphExprFacts,
    node: &SyntaxNode,
    owner: Option<&str>,
    params: &BTreeMap<String, std::collections::BTreeSet<String>>,
    syntax: &crate::loader::DatabaseMacroSyntax,
) {
    if let Some(function) = ast::Fn::cast(node.clone())
        && syntax::is_source_method(&function)
    {
        facts.events.push(GraphExprEvent::SourceMethod {
            owner: owner.unwrap_or_default().to_string(),
            argument: source_argument(&function),
        });
    }
    if let Some(record) = ast::RecordExpr::cast(node.clone())
        && let Some(name) = syntax::record_name(record.path(), owner)
    {
        let fields = record
            .record_expr_field_list()
            .into_iter()
            .flat_map(|list| list.fields())
            .filter_map(|field| {
                let expr = field.expr()?;
                let name = field.name_ref().or_else(|| match &expr {
                    ast::Expr::PathExpr(path) => path.path()?.segment()?.name_ref(),
                    _ => None,
                })?;
                // A numeric tuple field is not a named graph field.
                if name
                    .text()
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
                {
                    return None;
                }
                let value = value(&expr, params);
                Some((name.text().to_string(), value))
            })
            .collect();
        facts.events.push(GraphExprEvent::Struct {
            name,
            has_rest: record
                .record_expr_field_list()
                .is_some_and(|list| list.dotdot_token().is_some()),
            fields,
        });
    }
    if let Some(call) = ast::CallExpr::cast(node.clone())
        && let Some(ast::Expr::PathExpr(path)) = call.expr()
    {
        let names = path.path().map(names).unwrap_or_default();
        if let [.., owner, variant] = names.as_slice() {
            let args = call
                .arg_list()
                .into_iter()
                .flat_map(|list| list.args())
                .map(|arg| value(&arg, params))
                .collect();
            facts.events.push(GraphExprEvent::Variant {
                owner: owner.to_string(),
                variant: variant.clone(),
                args,
            });
        }
    }
    if let Some(call) = ast::MethodCallExpr::cast(node.clone())
        && syntax::method(&call, "as_ref")
        && call
            .receiver()
            .is_some_and(|expr| self_field(&expr, "kind"))
    {
        facts.events.push(GraphExprEvent::KindAsRef {
            owner: owner.unwrap_or_default().to_string(),
            receiver: call.receiver().map(|expr| range(expr.syntax())),
            whole: Some(range(call.syntax())),
        });
    }
    if let Some(expr) = ast::MatchExpr::cast(node.clone())
        && expr.expr().is_some_and(|expr| self_field(&expr, "kind"))
    {
        facts.events.push(GraphExprEvent::KindMatch {
            owner: owner.unwrap_or_default().to_string(),
            subject: expr.expr().map(|expr| range(expr.syntax())),
        });
    }
    if let Some(field) = ast::FieldExpr::cast(node.clone())
        && let Some(name) = field.name_ref().filter(|name| {
            !name
                .text()
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
        })
    {
        facts.events.push(GraphExprEvent::FieldUse {
            owner: owner.unwrap_or_default().to_string(),
            field: name.text().to_string(),
            self_base: field
                .expr()
                .is_some_and(|expr| syntax::bare_path(&expr, "self")),
            range: Some(range(field.syntax())),
        });
    }
    if let Some(pattern) = ast::RecordPat::cast(node.clone())
        && let Some(name) = syntax::record_name(pattern.path(), owner)
    {
        let fields = pattern
            .record_pat_field_list()
            .into_iter()
            .flat_map(|list| list.fields())
            .filter_map(|field| {
                field
                    .name_ref()
                    .map(|name| name.text().to_string())
                    .or_else(|| {
                        field.pat().and_then(|pat| match pat {
                            ast::Pat::IdentPat(pat) => {
                                pat.name().map(|name| name.text().to_string())
                            }
                            _ => None,
                        })
                    })
            })
            .filter(|name| !name.chars().next().is_some_and(|c| c.is_ascii_digit()))
            .collect();
        facts.events.push(GraphExprEvent::Pattern { name, fields });
    }
    if let Some(invocation) = ast::MacroCall::cast(node.clone()) {
        facts.events.push(macros::event(
            invocation.token_tree(),
            Some(&invocation),
            syntax,
        ));
    }
    if let Some(definition) = ast::MacroRules::cast(node.clone()) {
        facts
            .events
            .push(macros::event(definition.token_tree(), None, syntax));
    }
}

#[instrument(level = "trace", skip(node))]
fn range(node: &SyntaxNode) -> Range {
    let range = node.text_range();
    (range.start().into(), range.end().into())
}

#[instrument(level = "debug", skip(expr, params))]
fn value(
    expr: &ast::Expr,
    params: &BTreeMap<String, std::collections::BTreeSet<String>>,
) -> GraphValueFact {
    let method = match expr {
        ast::Expr::MethodCallExpr(call) => Some(call),
        _ => None,
    };
    let borrowed_string = method
        .filter(|call| {
            ["clone", "to_owned", "to_string"]
                .iter()
                .any(|name| syntax::method(call, name))
        })
        .and_then(|call| call.receiver())
        .map(|receiver| range(receiver.syntax()));
    let borrowed_value = method
        .filter(|call| syntax::method(call, "clone"))
        .and_then(|call| call.receiver())
        .map(|receiver| range(receiver.syntax()));
    let borrowed_boxed = match expr {
        ast::Expr::CallExpr(call) if syntax::boxed_call(call, false) => call
            .arg_list()
            .and_then(|list| list.args().next())
            .and_then(|expr| match expr {
                ast::Expr::MethodCallExpr(inner) if syntax::method(&inner, "clone") => {
                    inner.receiver()
                }
                _ => None,
            })
            .map(|receiver| range(receiver.syntax())),
        _ => None,
    };
    GraphValueFact {
        range: Some(range(expr.syntax())),
        borrowed_string,
        borrowed_value,
        borrowed_boxed,
        anchored_lifetimes: params
            .iter()
            .filter(|&(_lifetime, params)| syntax::origin(expr, params))
            .map(|(lifetime, _params)| lifetime.clone())
            .collect(),
    }
}

#[instrument(level = "debug", skip(function))]
fn source_argument(function: &ast::Fn) -> SourceArgument {
    let argument = function
        .body()
        .and_then(|body| body.stmt_list())
        .and_then(|list| list.tail_expr())
        .and_then(|expr| match expr {
            ast::Expr::CallExpr(call)
                if call
                    .expr()
                    .is_some_and(|expr| syntax::bare_path(&expr, "Some"))
                    && call.arg_list().is_some_and(|list| list.args().count() == 1) =>
            {
                call.arg_list().and_then(|list| list.args().next())
            }
            _ => None,
        });
    match argument {
        Some(ast::Expr::RefExpr(reference))
            if reference
                .expr()
                .is_some_and(|expr| self_field(&expr, "source")) =>
        {
            SourceArgument::BorrowedReference {
                field: reference.expr().map(|expr| range(expr.syntax())),
                reference: Some(range(reference.syntax())),
            }
        }
        Some(expr) if self_field(&expr, "source") => SourceArgument::OwnedField {
            field: Some(range(expr.syntax())),
        },
        _ => SourceArgument::Unsupported,
    }
}
