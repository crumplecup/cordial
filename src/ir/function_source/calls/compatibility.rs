//! Compatibility collection while the remaining syn consumers migrate.

use super::{FunctionCallSyntax, FunctionUnwrapSyntax, SourceByteRange, SourceCallUse};
use std::collections::BTreeSet;
use syn::{spanned::Spanned, visit::Visit};

#[tracing::instrument(level = "trace", skip(function))]
pub(in crate::ir::function_source) fn collect(
    function: &syn::ItemFn,
) -> (
    Vec<FunctionCallSyntax>,
    Vec<FunctionUnwrapSyntax>,
    bool,
    Vec<SourceByteRange>,
) {
    let mut visitor = Calls {
        calls: Vec::new(),
        unwraps: Vec::new(),
        has_shadowing: false,
        bindings: BTreeSet::new(),
        usage: SourceCallUse::Value,
        opaque_calls: Vec::new(),
    };
    for input in &function.sig.inputs {
        if let syn::FnArg::Typed(input) = input {
            visitor.visit_pat(&input.pat);
        }
    }
    visitor.visit_block(&function.block);
    // Conservatively reject a name bound anywhere in this function; later
    // lexical identities can recover calls outside that binding's scope.
    for call in &mut visitor.calls {
        if call
            .local_name
            .as_ref()
            .is_some_and(|name| visitor.bindings.contains(name))
        {
            call.local_name = None;
        }
    }
    (
        visitor.calls,
        visitor.unwraps,
        visitor.has_shadowing,
        visitor.opaque_calls,
    )
}

struct Calls {
    calls: Vec<FunctionCallSyntax>,
    unwraps: Vec<FunctionUnwrapSyntax>,
    has_shadowing: bool,
    bindings: BTreeSet<String>,
    usage: SourceCallUse,
    opaque_calls: Vec<SourceByteRange>,
}

impl Calls {
    #[tracing::instrument(level = "trace", skip(self, expression, usage))]
    fn visit_used(&mut self, expression: &syn::Expr, usage: SourceCallUse) -> bool {
        if !matches!(expression, syn::Expr::Call(_)) {
            self.visit_expr(expression);
            return false;
        }
        self.usage = usage;
        self.visit_expr(expression);
        true
    }
}

impl<'ast> Visit<'ast> for Calls {
    #[tracing::instrument(level = "trace", skip(self, call))]
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        let local_name = match call.func.as_ref() {
            syn::Expr::Path(path) if path.qself.is_none() && path.path.get_ident().is_some() => {
                path.path
                    .get_ident()
                    .map(|ident| ident.to_string().trim_start_matches("r#").to_owned())
            }
            _ => None,
        };
        self.calls.push(FunctionCallSyntax {
            range: call.span().byte_range().into(),
            local_name,
            usage: Some(self.usage),
        });
        // The outer operation handles only this call's result, never the
        // independent calls evaluated while producing its arguments.
        self.usage = SourceCallUse::Value;
        syn::visit::visit_expr_call(self, call);
    }

    #[tracing::instrument(level = "trace", skip(self, call))]
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if call.method == "unwrap" && call.args.is_empty() && call.turbofish.is_none() {
            self.unwraps.push(FunctionUnwrapSyntax {
                line: Some(call.span().start().line as u32),
                range: call.span().byte_range().into(),
                receiver: call.receiver.span().byte_range().into(),
                suffix_tokens: vec![
                    call.dot_token.span().byte_range().into(),
                    call.method.span().byte_range().into(),
                    call.paren_token.span.open().byte_range().into(),
                    call.paren_token.span.close().byte_range().into(),
                ],
            });
        }
        let usage = match (call.method.to_string().as_str(), call.args.len()) {
            ("unwrap", 0) => Some(SourceCallUse::Unwrap),
            ("unwrap_or", 1) | ("unwrap_or_default" | "is_ok" | "is_err", 0) => {
                Some(SourceCallUse::HandledResult)
            }
            _ => None,
        }
        .filter(|_| call.turbofish.is_none());
        let direct = if let Some(usage) = usage {
            self.visit_used(&call.receiver, usage)
        } else {
            self.visit_expr(&call.receiver);
            false
        };
        if !direct {
            self.opaque_calls.push(call.span().byte_range().into());
        }
        for argument in &call.args {
            self.visit_expr(argument);
        }
    }

    #[tracing::instrument(level = "trace", skip(self, expression))]
    fn visit_expr_try(&mut self, expression: &'ast syn::ExprTry) {
        if !self.visit_used(&expression.expr, SourceCallUse::Residual) {
            self.opaque_calls
                .push(expression.span().byte_range().into());
        }
    }

    #[tracing::instrument(level = "trace", skip(self, local))]
    fn visit_local(&mut self, local: &'ast syn::Local) {
        syn::visit::visit_local(self, local);
    }

    #[tracing::instrument(level = "trace", skip(self, pattern))]
    fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
        self.bindings.insert(pattern.ident.to_string());
        syn::visit::visit_pat_ident(self, pattern);
    }

    #[tracing::instrument(level = "trace", skip(self, _item))]
    fn visit_item(&mut self, _item: &'ast syn::Item) {
        self.has_shadowing = true;
    }

    #[tracing::instrument(level = "trace", skip(self, _expression))]
    fn visit_expr_closure(&mut self, _expression: &'ast syn::ExprClosure) {}

    #[tracing::instrument(level = "trace", skip(self, _expression))]
    fn visit_expr_async(&mut self, _expression: &'ast syn::ExprAsync) {}

    #[tracing::instrument(level = "trace", skip(self, _expression))]
    fn visit_expr_const(&mut self, _expression: &'ast syn::ExprConst) {}
}
