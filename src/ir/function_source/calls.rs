//! Located calls retained from the shared tree, without guessing receiver types.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use syn::{spanned::Spanned, visit::Visit};

use super::SourceByteRange;

/// A call expression belonging to this function's execution scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct FunctionCallSyntax {
    range: SourceByteRange,
    /// Only an unqualified, nongeneric path is eligible for the first resolver.
    local_name: Option<String>,
}

/// A zero-argument, nongeneric `unwrap` candidate; this does not prove Result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct FunctionUnwrapSyntax {
    range: SourceByteRange,
    receiver: SourceByteRange,
    /// Dot, method name, and delimiters; comments between them are retained.
    suffix_tokens: Vec<SourceByteRange>,
}

#[tracing::instrument(level = "trace", skip(function))]
pub(super) fn collect(
    function: &syn::ItemFn,
) -> (Vec<FunctionCallSyntax>, Vec<FunctionUnwrapSyntax>, bool) {
    let mut visitor = Calls {
        calls: Vec::new(),
        unwraps: Vec::new(),
        has_shadowing: false,
        bindings: BTreeSet::new(),
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
    (visitor.calls, visitor.unwraps, visitor.has_shadowing)
}

struct Calls {
    calls: Vec<FunctionCallSyntax>,
    unwraps: Vec<FunctionUnwrapSyntax>,
    has_shadowing: bool,
    bindings: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for Calls {
    #[tracing::instrument(level = "trace", skip(self, call))]
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        let local_name = match call.func.as_ref() {
            syn::Expr::Path(path) if path.qself.is_none() && path.path.get_ident().is_some() => {
                path.path.get_ident().map(ToString::to_string)
            }
            _ => None,
        };
        self.calls.push(FunctionCallSyntax {
            range: call.span().byte_range().into(),
            local_name,
        });
        syn::visit::visit_expr_call(self, call);
    }

    #[tracing::instrument(level = "trace", skip(self, call))]
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if call.method == "unwrap" && call.args.is_empty() && call.turbofish.is_none() {
            self.unwraps.push(FunctionUnwrapSyntax {
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
        syn::visit::visit_expr_method_call(self, call);
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
