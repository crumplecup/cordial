//! Structural exits in the canonical syntax tree.

use ra_ap_syntax::{AstNode, ast};

use super::FunctionSuccessExit;

#[tracing::instrument(level = "trace", skip(block, exits))]
pub(in crate::ir::function_source) fn collect_block(
    block: &ast::BlockExpr,
    exits: &mut Vec<FunctionSuccessExit>,
) {
    let Some(statements) = block.stmt_list() else {
        return;
    };
    if let Some(tail) = statements.tail_expr() {
        collect_value(&tail, exits);
    } else if !statements.statements().last().is_some_and(|statement| {
        matches!(statement, ast::Stmt::ExprStmt(statement) if statement.expr().as_ref().is_some_and(always_returns))
    }) {
        if let Some(close) = statements.r_curly_token() {
            exits.push(FunctionSuccessExit::BlockUnit { offset: usize::from(close.text_range().start()) });
        }
    }
}

#[tracing::instrument(level = "trace", skip(expression, exits))]
pub(in crate::ir::function_source) fn collect_value(
    expression: &ast::Expr,
    exits: &mut Vec<FunctionSuccessExit>,
) {
    match expression {
        ast::Expr::ReturnExpr(_) => {}
        ast::Expr::IfExpr(branch) => {
            if let Some(block) = branch.then_branch() {
                collect_block(&block, exits);
            }
            match branch.else_branch() {
                Some(ast::ElseBranch::Block(block)) => collect_block(&block, exits),
                Some(ast::ElseBranch::IfExpr(branch)) => {
                    collect_value(&ast::Expr::IfExpr(branch), exits)
                }
                None => exits.push(FunctionSuccessExit::MissingElse {
                    offset: usize::from(branch.syntax().text_range().end()),
                }),
            }
        }
        ast::Expr::MatchExpr(branches) => {
            if let Some(arms) = branches.match_arm_list() {
                for value in arms.arms().filter_map(|arm| arm.expr()) {
                    collect_value(&value, exits);
                }
            }
        }
        ast::Expr::BlockExpr(block)
            if block.async_token().is_none() && block.const_token().is_none() =>
        {
            collect_block(block, exits)
        }
        ast::Expr::ParenExpr(value) => {
            if let Some(value) = value.expr() {
                collect_value(&value, exits);
            }
        }
        _ => exits.push(FunctionSuccessExit::Expression(
            std::ops::Range::<usize>::from(expression.syntax().text_range()).into(),
        )),
    }
}

#[tracing::instrument(level = "trace", skip(expression))]
fn always_returns(expression: &ast::Expr) -> bool {
    match expression {
        ast::Expr::ReturnExpr(_) => true,
        ast::Expr::IfExpr(branch) => {
            branch.then_branch().as_ref().is_some_and(block_returns)
                && match branch.else_branch() {
                    Some(ast::ElseBranch::Block(block)) => block_returns(&block),
                    Some(ast::ElseBranch::IfExpr(branch)) => {
                        always_returns(&ast::Expr::IfExpr(branch))
                    }
                    None => false,
                }
        }
        ast::Expr::MatchExpr(branch) => branch.match_arm_list().is_some_and(|arms| {
            arms.arms().next().is_some()
                && arms
                    .arms()
                    .all(|arm| arm.expr().as_ref().is_some_and(always_returns))
        }),
        ast::Expr::BlockExpr(block)
            if block.async_token().is_none() && block.const_token().is_none() =>
        {
            block_returns(block)
        }
        ast::Expr::ParenExpr(value) => value.expr().as_ref().is_some_and(always_returns),
        _ => false,
    }
}

#[tracing::instrument(level = "trace", skip(block))]
fn block_returns(block: &ast::BlockExpr) -> bool {
    block.stmt_list().is_some_and(|statements| {
        statements.statements().any(|statement| matches!(statement, ast::Stmt::ExprStmt(statement) if statement.expr().as_ref().is_some_and(always_returns)))
            || statements.tail_expr().as_ref().is_some_and(always_returns)
    })
}
