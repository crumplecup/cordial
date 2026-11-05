//! Match the existing error-value assertion exemption from database syntax.
use crate::loader::DatabaseSource;
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode, ast, ast::HasName};
use std::collections::BTreeSet;

#[tracing::instrument(level = "debug", skip(file, block))]
pub(super) fn error_assertion_lines(
    file: &DatabaseSource<'_>,
    block: &ast::BlockExpr,
) -> BTreeSet<u32> {
    let Some(list) = block.stmt_list() else {
        return BTreeSet::new();
    };
    let mut statements: Vec<_> = list
        .statements()
        .map(|stmt| stmt.syntax().clone())
        .collect();
    statements.extend(list.tail_expr().map(|expr| expr.syntax().clone()));
    let mut exempt = BTreeSet::new();
    for (index, node) in statements.iter().enumerate() {
        let Some((name, init)) = binding(node) else {
            continue;
        };
        let ast::Expr::MethodCallExpr(call) = init else {
            continue;
        };
        if !call
            .name_ref()
            .is_some_and(|name| matches!(name.text(), "unwrap_err" | "expect_err"))
        {
            continue;
        }
        let mut reachable = BTreeSet::from([name]);
        for later in &statements[index + 1..] {
            if let Some((name, init)) = binding(later) {
                if identifiers(init.syntax())
                    .iter()
                    .any(|name| reachable.contains(name))
                {
                    reachable.insert(name);
                }
                continue;
            }
            let invocation = ast::MacroCall::cast(later.clone()).or_else(|| {
                let expr = ast::ExprStmt::cast(later.clone())
                    .and_then(|stmt| stmt.expr())
                    .or_else(|| ast::Expr::cast(later.clone()))?;
                let ast::Expr::MacroExpr(expr) = expr else {
                    return None;
                };
                expr.macro_call()
            });
            if let Some(invocation) = invocation
                && invocation
                    .path()
                    .as_ref()
                    .and_then(super::syntax::path_name)
                    .is_some_and(|name| {
                        matches!(
                            name.as_str(),
                            "assert"
                                | "assert_eq"
                                | "assert_ne"
                                | "debug_assert"
                                | "debug_assert_eq"
                                | "debug_assert_ne"
                        )
                    })
                && invocation.token_tree().is_some_and(|tree| {
                    identifiers(tree.syntax())
                        .iter()
                        .any(|name| reachable.contains(name))
                })
            {
                exempt.insert(file.line(call.syntax().text_range().start()));
                break;
            }
        }
    }
    exempt
}

#[tracing::instrument(level = "trace", skip(node))]
fn binding(node: &SyntaxNode) -> Option<(String, ast::Expr)> {
    let local = ast::LetStmt::cast(node.clone())?;
    let ast::Pat::IdentPat(pat) = local.pat()? else {
        return None;
    };
    Some((pat.name()?.text().to_string(), local.initializer()?))
}

#[tracing::instrument(level = "trace", skip(node))]
fn identifiers(node: &SyntaxNode) -> BTreeSet<String> {
    node.descendants_with_tokens()
        .filter_map(|part| part.into_token())
        .filter(|token| token.kind() == SyntaxKind::IDENT)
        .map(|token| token.text().to_string())
        .collect()
}
