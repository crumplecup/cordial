//! Structural value exits, collected from the shared tree before planning.

use serde::{Deserialize, Serialize};
use syn::spanned::Spanned;

use super::SourceByteRange;

pub(super) mod native;

/// A successful exit that must acquire the lifted function's Result wrapper.
///
/// These positions belong to the enclosing source snapshot. They describe
/// syntax, not inferred types or caller closure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionSuccessExit {
    /// Value evaluated once at a successful exit.
    Expression(SourceByteRange),
    /// Bare return; insert its unit operand after the keyword.
    BareReturn {
        /// Byte immediately after `return`.
        offset: usize,
    },
    /// Statement-only block falls through with unit.
    BlockUnit {
        /// Byte immediately before the block's closing brace.
        offset: usize,
    },
    /// An if without an else supplies unit on its untaken path.
    MissingElse {
        /// Byte immediately after the if expression.
        offset: usize,
    },
}

#[tracing::instrument(level = "trace", skip(block, exits))]
pub(super) fn collect_block(block: &syn::Block, exits: &mut Vec<FunctionSuccessExit>) {
    match block.stmts.last() {
        Some(syn::Stmt::Expr(expression, None)) => collect_value(expression, exits),
        Some(syn::Stmt::Expr(expression, Some(_))) if always_returns(expression) => {}
        _ => exits.push(FunctionSuccessExit::BlockUnit {
            offset: block.brace_token.span.close().byte_range().start,
        }),
    }
}

#[tracing::instrument(level = "trace", skip(expression, exits))]
pub(super) fn collect_value(expression: &syn::Expr, exits: &mut Vec<FunctionSuccessExit>) {
    match expression {
        syn::Expr::Return(_) => {} // The return visitor collects its operand.
        syn::Expr::If(branch) => {
            collect_block(&branch.then_branch, exits);
            match &branch.else_branch {
                Some((_, value)) => collect_value(value, exits),
                None => exits.push(FunctionSuccessExit::MissingElse {
                    offset: branch.span().byte_range().end,
                }),
            }
        }
        syn::Expr::Match(branches) => {
            for arm in &branches.arms {
                collect_value(&arm.body, exits);
            }
        }
        syn::Expr::Block(block) => collect_block(&block.block, exits),
        syn::Expr::Unsafe(block) => collect_block(&block.block, exits),
        syn::Expr::Paren(value) => collect_value(&value.expr, exits),
        syn::Expr::Group(value) => collect_value(&value.expr, exits),
        _ => exits.push(FunctionSuccessExit::Expression(
            expression.span().byte_range().into(),
        )),
    }
}

// Recognize syntactically terminal statements so a semicolon after a returning
// branch does not cause an unreachable synthetic unit expression. No call's
// return type is inferred here; loops/macros already have explicit boundaries.
#[tracing::instrument(level = "trace", skip(expression))]
fn always_returns(expression: &syn::Expr) -> bool {
    match expression {
        syn::Expr::Return(_) => true,
        syn::Expr::If(branch) => {
            block_returns(&branch.then_branch)
                && branch
                    .else_branch
                    .as_ref()
                    .is_some_and(|(_, value)| always_returns(value))
        }
        syn::Expr::Match(branches) => {
            !branches.arms.is_empty() && branches.arms.iter().all(|arm| always_returns(&arm.body))
        }
        syn::Expr::Block(block) => block_returns(&block.block),
        syn::Expr::Unsafe(block) => block_returns(&block.block),
        syn::Expr::Paren(value) => always_returns(&value.expr),
        syn::Expr::Group(value) => always_returns(&value.expr),
        _ => false,
    }
}

#[tracing::instrument(level = "trace", skip(block))]
fn block_returns(block: &syn::Block) -> bool {
    block.stmts.iter().any(|statement| {
        matches!(statement, syn::Stmt::Expr(expression, _) if always_returns(expression))
    })
}
