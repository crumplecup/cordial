//! Returns and transformation boundaries collected from the database tree.

use ra_ap_syntax::{
    AstNode, WalkEvent,
    ast::{self, HasAttrs, HasGenericParams, HasVisibility},
};

use super::{
    FunctionBoundary, FunctionNestedScope, FunctionReturn, FunctionReturnFlow, FunctionSuccessExit,
    FunctionTail, NestedScopeKind, ReturnValue,
};
use crate::ir::function_source::success;

impl FunctionReturnFlow {
    #[tracing::instrument(level = "trace", skip(function))]
    pub(in crate::ir::function_source) fn collect_native(function: &ast::Fn) -> Self {
        let mut flow = Self {
            returns: Vec::new(),
            tail: FunctionTail::Statements,
            success_exits: Vec::new(),
            nested_scopes: Vec::new(),
            boundaries: Vec::new(),
        };
        flow.collect_signature(function);
        if let Some(body) = function.body() {
            flow.collect_body(&body);
        }
        flow
    }

    #[tracing::instrument(level = "trace", skip(self, function))]
    fn collect_signature(&mut self, function: &ast::Fn) {
        if function.visibility().is_some() {
            self.native_boundary(FunctionBoundary::Visible);
        }
        if function.const_token().is_some()
            || function.async_token().is_some()
            || function.unsafe_token().is_some()
            || function.abi().is_some()
            || function.param_list().is_some_and(|params| {
                params
                    .params()
                    .any(|param| param.dotdotdot_token().is_some())
            })
        {
            self.native_boundary(FunctionBoundary::SpecialSignature);
        }
        if function.generic_param_list().is_some() || function.where_clause().is_some() {
            self.native_boundary(FunctionBoundary::Generic);
        }
        if matches!(
            function.ret_type().and_then(|output| output.ty()),
            Some(ast::Type::NeverType(_))
        ) {
            self.native_boundary(FunctionBoundary::NeverOutput);
        }
        for attr in function.attrs() {
            if !attr.simple_name().is_some_and(|name| name == "doc") {
                self.native_boundary(FunctionBoundary::Attribute);
            }
        }
    }

    #[tracing::instrument(level = "trace", skip(self, body))]
    fn collect_body(&mut self, body: &ast::BlockExpr) {
        let mut walk = body.syntax().preorder();
        while let Some(event) = walk.next() {
            let WalkEvent::Enter(node) = event else {
                continue;
            };
            let nested = nested_scope_kind(&node);
            if let Some(kind) = nested {
                self.native_boundary(FunctionBoundary::NestedScope);
                self.nested_scopes.push(FunctionNestedScope {
                    range: std::ops::Range::<usize>::from(node.text_range()).into(),
                    kind,
                });
                walk.skip_subtree();
                continue;
            }
            if let Some(expression) = ast::ReturnExpr::cast(node.clone()) {
                self.collect_return(&expression);
            }
            if ast::MacroCall::can_cast(node.kind()) {
                self.native_boundary(FunctionBoundary::Macro);
                walk.skip_subtree();
            }
            if ast::TryExpr::can_cast(node.kind()) {
                self.native_boundary(FunctionBoundary::Try);
            }
            if ast::LoopExpr::can_cast(node.kind())
                || ast::ForExpr::can_cast(node.kind())
                || ast::WhileExpr::can_cast(node.kind())
                || ast::BreakExpr::can_cast(node.kind())
            {
                self.native_boundary(FunctionBoundary::LoopControl);
            }
            if let Some(attr) = ast::Attr::cast(node)
                && !attr.simple_name().is_some_and(|name| name == "doc")
            {
                self.native_boundary(FunctionBoundary::Attribute);
            }
        }
        success::native::collect_block(body, &mut self.success_exits);
        self.tail = body_tail(body);
    }

    #[tracing::instrument(level = "trace", skip(self, expression))]
    fn collect_return(&mut self, expression: &ast::ReturnExpr) {
        let value = if let Some(value) = expression.expr() {
            success::native::collect_value(&value, &mut self.success_exits);
            ReturnValue::Expression(
                std::ops::Range::<usize>::from(value.syntax().text_range()).into(),
            )
        } else {
            self.success_exits.push(FunctionSuccessExit::BareReturn {
                offset: usize::from(expression.syntax().text_range().end()),
            });
            ReturnValue::Unit
        };
        self.returns.push(FunctionReturn {
            range: std::ops::Range::<usize>::from(expression.syntax().text_range()).into(),
            value,
        });
    }

    #[tracing::instrument(level = "trace", skip(self, boundary))]
    fn native_boundary(&mut self, boundary: FunctionBoundary) {
        if !self.boundaries.contains(&boundary) {
            self.boundaries.push(boundary);
        }
    }
}

#[tracing::instrument(level = "trace", skip(node))]
fn nested_scope_kind(node: &ra_ap_syntax::SyntaxNode) -> Option<NestedScopeKind> {
    if ast::Item::can_cast(node.kind()) && !ast::MacroCall::can_cast(node.kind()) {
        Some(NestedScopeKind::Item)
    } else if ast::ClosureExpr::can_cast(node.kind()) {
        Some(NestedScopeKind::Closure)
    } else if let Some(block) = ast::BlockExpr::cast(node.clone()) {
        if block.async_token().is_some() {
            Some(NestedScopeKind::Async)
        } else if block.const_token().is_some() {
            Some(NestedScopeKind::Const)
        } else {
            None
        }
    } else {
        None
    }
}

#[tracing::instrument(level = "trace", skip(body))]
fn body_tail(body: &ast::BlockExpr) -> FunctionTail {
    let Some(statements) = body.stmt_list() else {
        return FunctionTail::Statements;
    };
    match statements.tail_expr() {
        Some(ast::Expr::ReturnExpr(_)) => FunctionTail::Return,
        Some(expression) => FunctionTail::Expression(
            std::ops::Range::<usize>::from(expression.syntax().text_range()).into(),
        ),
        None if matches!(statements.statements().last(), Some(ast::Stmt::ExprStmt(statement)) if matches!(statement.expr(), Some(ast::Expr::ReturnExpr(_)))) => {
            FunctionTail::Return
        }
        None => FunctionTail::Statements,
    }
}
