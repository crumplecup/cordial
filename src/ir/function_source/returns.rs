//! Return paths and boundaries collected during the shared source walk.

use serde::{Deserialize, Serialize};
use syn::spanned::Spanned;
use syn::visit::Visit;

use super::SourceByteRange;

/// A return exits the current function, rather than a nested execution scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct FunctionReturn {
    /// Whole return expression, including its keyword.
    range: SourceByteRange,
    /// A bare return produces unit; otherwise retain the exact value range.
    value: ReturnValue,
}

/// Value returned explicitly from a function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReturnValue {
    /// `return` without an operand.
    Unit,
    /// Operand evaluated once when this return is reached.
    Expression(SourceByteRange),
}

/// How control reaches the end of a function body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionTail {
    /// A statement-only block falls through with unit.
    Unit,
    /// An explicit return is already represented in the return inventory.
    Return,
    /// A trailing expression supplies the value.
    Expression(SourceByteRange),
}

/// Syntax requiring additional proof before changing a function contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionBoundary {
    /// Externally visible callers have not been inventoried.
    Visible,
    /// Const, async, unsafe, ABI, or variadic contracts need dedicated handling.
    SpecialSignature,
    /// Generic contracts need type and lifetime substitution evidence.
    Generic,
    /// Attributes may impose cfg, macro, or external signature contracts.
    Attribute,
    /// Macro expansion may contain additional exits or depend on return types.
    Macro,
    /// Existing residual propagation needs a resolved conversion contract.
    Try,
    /// Nested scopes have separate returns and capture contracts.
    NestedScope,
}

/// A nested body whose returns do not belong to the enclosing function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct FunctionNestedScope {
    range: SourceByteRange,
    #[getter(copy)]
    kind: NestedScopeKind,
}

/// Execution boundaries within a function body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NestedScopeKind {
    /// Closure, with its own return contract.
    Closure,
    /// Async block, returning into its future.
    Async,
    /// Local item, whose body is not executed as part of the enclosing body.
    Item,
    /// Inline const evaluation has a distinct execution context.
    Const,
}

/// Complete syntactic return inventory for one collected function body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct FunctionReturnFlow {
    returns: Vec<FunctionReturn>,
    tail: FunctionTail,
    nested_scopes: Vec<FunctionNestedScope>,
    boundaries: Vec<FunctionBoundary>,
}

impl FunctionReturnFlow {
    #[tracing::instrument(level = "trace", skip(function))]
    pub(super) fn collect(function: &syn::ItemFn) -> Self {
        let mut collector = ReturnCollector {
            returns: Vec::new(),
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
        for attribute in &function.attrs {
            collector.visit_attribute(attribute);
        }
        collector.visit_block(&function.block);
        let tail = match function.block.stmts.last() {
            Some(syn::Stmt::Expr(syn::Expr::Return(_), _)) => FunctionTail::Return,
            Some(syn::Stmt::Expr(expression, None)) => {
                FunctionTail::Expression(expression.span().byte_range().into())
            }
            _ => FunctionTail::Unit,
        };
        Self {
            returns: collector.returns,
            tail,
            nested_scopes: collector.nested_scopes,
            boundaries: collector.boundaries,
        }
    }
}

struct ReturnCollector {
    returns: Vec<FunctionReturn>,
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

    #[tracing::instrument(level = "trace", skip(self, _attribute))]
    fn visit_attribute(&mut self, _attribute: &'ast syn::Attribute) {
        self.boundary(FunctionBoundary::Attribute);
    }

    #[tracing::instrument(level = "trace", skip(self, _syntax))]
    fn visit_macro(&mut self, _syntax: &'ast syn::Macro) {
        self.boundary(FunctionBoundary::Macro);
    }

    #[tracing::instrument(level = "trace", skip(self, expression))]
    fn visit_expr_try(&mut self, expression: &'ast syn::ExprTry) {
        self.boundary(FunctionBoundary::Try);
        syn::visit::visit_expr_try(self, expression);
    }
}
