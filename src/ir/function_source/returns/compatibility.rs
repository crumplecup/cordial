//! Compatibility collection while the remaining syn consumers migrate.

use super::{
    FunctionBoundary, FunctionNestedScope, FunctionReturn, FunctionReturnFlow, FunctionSuccessExit,
    FunctionTail, NestedScopeKind, ReturnValue,
};
use crate::ir::function_source::success;
use syn::{spanned::Spanned, visit::Visit};

impl FunctionReturnFlow {
    #[tracing::instrument(level = "trace", skip(function))]
    pub(in crate::ir::function_source) fn collect(function: &syn::ItemFn) -> Self {
        let mut collector = ReturnCollector {
            returns: Vec::new(),
            success_exits: Vec::new(),
            nested_scopes: Vec::new(),
            boundaries: Vec::new(),
        };
        if !matches!(function.vis, syn::Visibility::Inherited) {
            collector.boundary(FunctionBoundary::Visible);
        }
        let signature = &function.sig;
        if signature.constness.is_some()
            || signature.asyncness.is_some()
            || matches!(signature.safety, syn::Safety::Unsafe(_))
            || signature.abi.is_some()
            || signature.variadic.is_some()
        {
            collector.boundary(FunctionBoundary::SpecialSignature);
        }
        if !signature.generics.params.is_empty() || signature.generics.where_clause.is_some() {
            collector.boundary(FunctionBoundary::Generic);
        }
        if matches!(&signature.output, syn::ReturnType::Type(_, ty) if matches!(ty.as_ref(), syn::Type::Never(_)))
        {
            collector.boundary(FunctionBoundary::NeverOutput);
        }
        for attribute in &function.attrs {
            collector.visit_attribute(attribute);
        }
        collector.visit_block(&function.block);
        success::collect_block(&function.block, &mut collector.success_exits);
        let tail = match function.block.stmts.last() {
            Some(syn::Stmt::Expr(syn::Expr::Return(_), _)) => FunctionTail::Return,
            Some(syn::Stmt::Expr(expression, None)) => {
                FunctionTail::Expression(expression.span().byte_range().into())
            }
            _ => FunctionTail::Statements,
        };
        Self {
            returns: collector.returns,
            tail,
            success_exits: collector.success_exits,
            nested_scopes: collector.nested_scopes,
            boundaries: collector.boundaries,
        }
    }
}

struct ReturnCollector {
    returns: Vec<FunctionReturn>,
    success_exits: Vec<FunctionSuccessExit>,
    nested_scopes: Vec<FunctionNestedScope>,
    boundaries: Vec<FunctionBoundary>,
}

impl ReturnCollector {
    #[tracing::instrument(level = "trace", skip(self, boundary))]
    fn boundary(&mut self, boundary: FunctionBoundary) {
        if !self.boundaries.contains(&boundary) {
            self.boundaries.push(boundary);
        }
    }

    #[tracing::instrument(level = "trace", skip(self, kind, span))]
    fn nested(&mut self, kind: NestedScopeKind, span: proc_macro2::Span) {
        self.boundary(FunctionBoundary::NestedScope);
        self.nested_scopes.push(FunctionNestedScope {
            kind,
            range: span.byte_range().into(),
        });
    }
}

impl<'ast> Visit<'ast> for ReturnCollector {
    #[tracing::instrument(level = "trace", skip(self, expression))]
    fn visit_expr_return(&mut self, expression: &'ast syn::ExprReturn) {
        match &expression.expr {
            Some(value) => success::collect_value(value, &mut self.success_exits),
            None => self.success_exits.push(FunctionSuccessExit::BareReturn {
                offset: expression.span().byte_range().end,
            }),
        }
        self.returns.push(FunctionReturn {
            range: expression.span().byte_range().into(),
            value: match &expression.expr {
                Some(value) => ReturnValue::Expression(value.span().byte_range().into()),
                None => ReturnValue::Unit,
            },
        });
        syn::visit::visit_expr_return(self, expression);
    }

    #[tracing::instrument(level = "trace", skip(self, expression))]
    fn visit_expr_closure(&mut self, expression: &'ast syn::ExprClosure) {
        self.nested(NestedScopeKind::Closure, expression.span());
    }

    #[tracing::instrument(level = "trace", skip(self, expression))]
    fn visit_expr_async(&mut self, expression: &'ast syn::ExprAsync) {
        self.nested(NestedScopeKind::Async, expression.span());
    }

    #[tracing::instrument(level = "trace", skip(self, expression))]
    fn visit_expr_const(&mut self, expression: &'ast syn::ExprConst) {
        self.nested(NestedScopeKind::Const, expression.span());
    }

    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_item(&mut self, item: &'ast syn::Item) {
        self.nested(NestedScopeKind::Item, item.span());
    }

    #[tracing::instrument(level = "trace", skip(self, attribute))]
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        if !attribute.path().is_ident("doc") {
            self.boundary(FunctionBoundary::Attribute);
        }
    }

    #[tracing::instrument(level = "trace", skip(self, _syntax))]
    fn visit_macro(&mut self, _syntax: &'ast syn::Macro) {
        self.boundary(FunctionBoundary::Macro);
    }

    #[tracing::instrument(level = "trace", skip(self, expression))]
    fn visit_expr_loop(&mut self, expression: &'ast syn::ExprLoop) {
        self.boundary(FunctionBoundary::LoopControl);
        syn::visit::visit_expr_loop(self, expression);
    }

    #[tracing::instrument(level = "trace", skip(self, expression))]
    fn visit_expr_for_loop(&mut self, expression: &'ast syn::ExprForLoop) {
        self.boundary(FunctionBoundary::LoopControl);
        syn::visit::visit_expr_for_loop(self, expression);
    }

    #[tracing::instrument(level = "trace", skip(self, expression))]
    fn visit_expr_while(&mut self, expression: &'ast syn::ExprWhile) {
        self.boundary(FunctionBoundary::LoopControl);
        syn::visit::visit_expr_while(self, expression);
    }

    #[tracing::instrument(level = "trace", skip(self, expression))]
    fn visit_expr_break(&mut self, expression: &'ast syn::ExprBreak) {
        self.boundary(FunctionBoundary::LoopControl);
        syn::visit::visit_expr_break(self, expression);
    }

    #[tracing::instrument(level = "trace", skip(self, expression))]
    fn visit_expr_try(&mut self, expression: &'ast syn::ExprTry) {
        self.boundary(FunctionBoundary::Try);
        syn::visit::visit_expr_try(self, expression);
    }
}
