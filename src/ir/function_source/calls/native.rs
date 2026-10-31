//! Call syntax from the database tree; semantic resolution is a later stage.

use std::collections::BTreeSet;

use ra_ap_syntax::{
    AstNode, WalkEvent,
    ast::{self, HasArgList, HasGenericArgs, HasName},
};

use super::{FunctionCallSyntax, FunctionUnwrapSyntax, SourceByteRange, SourceCallUse};
use crate::loader::DatabaseSource;

#[tracing::instrument(level = "trace", skip(file, function))]
pub(in crate::ir::function_source) fn collect(
    file: &DatabaseSource<'_>,
    function: &ast::Fn,
) -> (
    Vec<FunctionCallSyntax>,
    Vec<FunctionUnwrapSyntax>,
    bool,
    Vec<SourceByteRange>,
) {
    let mut calls = Vec::new();
    let mut unwraps = Vec::new();
    let mut bindings = BTreeSet::new();
    let mut has_shadowing = false;
    let mut opaque_calls = Vec::new();
    for pattern in function
        .param_list()
        .into_iter()
        .flat_map(|parameters| parameters.params())
        .filter_map(|parameter| parameter.pat())
    {
        for binding in pattern
            .syntax()
            .descendants()
            .filter_map(ast::IdentPat::cast)
            .filter_map(|pattern| pattern.name())
        {
            bindings.insert(binding.text().trim_start_matches("r#").to_owned());
        }
    }
    let Some(body) = function.body() else {
        return (calls, unwraps, has_shadowing, opaque_calls);
    };
    let mut walk = body.syntax().preorder();
    while let Some(event) = walk.next() {
        let node = match event {
            WalkEvent::Enter(node) => node,
            WalkEvent::Leave(node) => {
                record_receiver_operation(&node, &mut opaque_calls);
                continue;
            }
        };
        if ast::Item::can_cast(node.kind()) && !ast::MacroCall::can_cast(node.kind()) {
            has_shadowing = true;
            walk.skip_subtree();
            continue;
        }
        if ast::ClosureExpr::can_cast(node.kind()) || separate_block(&node) {
            walk.skip_subtree();
            continue;
        }
        if let Some(binding) = ast::IdentPat::cast(node.clone()).and_then(|pat| pat.name()) {
            bindings.insert(binding.text().trim_start_matches("r#").to_owned());
        }
        if let Some(call) = ast::CallExpr::cast(node.clone()) {
            calls.push(FunctionCallSyntax {
                range: std::ops::Range::<usize>::from(node.text_range()).into(),
                local_name: local_name(&call),
                usage: Some(call_usage(&call)),
            });
        }
        if let Some(method) = ast::MethodCallExpr::cast(node.clone()) {
            if let Some(candidate) = unwrap_candidate(file, &method) {
                unwraps.push(candidate);
            }
        }
    }
    for call in &mut calls {
        if call
            .local_name
            .as_ref()
            .is_some_and(|name| bindings.contains(name))
        {
            call.local_name = None;
        }
    }
    (calls, unwraps, has_shadowing, opaque_calls)
}

/// Record operations after their receiver, before evaluating their arguments.
#[tracing::instrument(level = "trace", skip(node, opaque_calls))]
fn record_receiver_operation(
    node: &ra_ap_syntax::SyntaxNode,
    opaque_calls: &mut Vec<SourceByteRange>,
) {
    let Some(parent) = node.parent() else {
        return;
    };
    let opaque = if let Some(method) = ast::MethodCallExpr::cast(parent.clone()) {
        method.receiver().as_ref().map(AstNode::syntax) == Some(node)
            && (method_usage(&method).is_none() || !ast::CallExpr::can_cast(node.kind()))
    } else if let Some(residual) = ast::TryExpr::cast(parent.clone()) {
        residual.expr().as_ref().map(AstNode::syntax) == Some(node)
            && !ast::CallExpr::can_cast(node.kind())
    } else {
        false
    };
    if opaque {
        opaque_calls.push(std::ops::Range::<usize>::from(parent.text_range()).into());
    }
}

#[tracing::instrument(level = "trace", skip(node))]
pub(in crate::ir::function_source) fn separate_block(node: &ra_ap_syntax::SyntaxNode) -> bool {
    ast::BlockExpr::cast(node.clone())
        .is_some_and(|block| block.async_token().is_some() || block.const_token().is_some())
}

#[tracing::instrument(level = "trace", skip(call))]
fn local_name(call: &ast::CallExpr) -> Option<String> {
    let ast::Expr::PathExpr(expression) = call.expr()? else {
        return None;
    };
    let path = expression.path()?;
    if path.qualifier().is_some() {
        return None;
    }
    let segment = path.segment()?;
    if segment.generic_arg_list().is_some()
        || segment.type_anchor().is_some()
        || segment.coloncolon_token().is_some()
    {
        return None;
    }
    Some(
        segment
            .name_ref()?
            .text()
            .trim_start_matches("r#")
            .to_owned(),
    )
}

#[tracing::instrument(level = "trace", skip(call))]
fn call_usage(call: &ast::CallExpr) -> SourceCallUse {
    let Some(parent) = call.syntax().parent() else {
        return SourceCallUse::Value;
    };
    if let Some(method) = ast::MethodCallExpr::cast(parent.clone()) {
        if method.receiver().as_ref().map(AstNode::syntax) == Some(call.syntax()) {
            return method_usage(&method).unwrap_or(SourceCallUse::Value);
        }
    }
    if ast::TryExpr::can_cast(parent.kind()) {
        SourceCallUse::Residual
    } else {
        SourceCallUse::Value
    }
}

#[tracing::instrument(level = "trace", skip(method))]
fn method_usage(method: &ast::MethodCallExpr) -> Option<SourceCallUse> {
    if method.generic_arg_list().is_some() {
        return None;
    }
    let name = method.name_ref()?;
    let count = method.arg_list()?.args().count();
    match (&*name.text(), count) {
        ("unwrap", 0) => Some(SourceCallUse::Unwrap),
        ("unwrap_or", 1) | ("unwrap_or_default" | "is_ok" | "is_err", 0) => {
            Some(SourceCallUse::HandledResult)
        }
        _ => None,
    }
}

#[tracing::instrument(level = "trace", skip(file, method))]
fn unwrap_candidate(
    file: &DatabaseSource<'_>,
    method: &ast::MethodCallExpr,
) -> Option<FunctionUnwrapSyntax> {
    if method_usage(method) != Some(SourceCallUse::Unwrap) {
        return None;
    }
    let arguments = method.arg_list()?;
    let tokens = [
        method.dot_token()?,
        method.name_ref()?.syntax().first_token()?,
        arguments.l_paren_token()?,
        arguments.r_paren_token()?,
    ];
    Some(FunctionUnwrapSyntax {
        line: Some(file.line(method.syntax().text_range().start())),
        range: std::ops::Range::<usize>::from(method.syntax().text_range()).into(),
        receiver: std::ops::Range::<usize>::from(method.receiver()?.syntax().text_range()).into(),
        suffix_tokens: tokens
            .into_iter()
            .map(|token| std::ops::Range::<usize>::from(token.text_range()).into())
            .collect(),
    })
}
