//! Conservative macro guards, using only loader-issued argument syntax.
use super::super::GraphExprEvent;
use super::syntax;
use crate::loader::DatabaseMacroSyntax;
use ra_ap_syntax::{AstNode, NodeOrToken, SyntaxElement, SyntaxKind, ast};
use std::collections::BTreeSet;
use tracing::instrument;

#[derive(Default)]
struct Tokens {
    identifiers: BTreeSet<String>,
    constructed: BTreeSet<String>,
    accessed_fields: BTreeSet<String>,
}

impl Tokens {
    #[instrument(level = "trace", skip(self, tree))]
    fn visit(&mut self, tree: &ast::TokenTree) {
        let parts: Vec<_> = tree
            .syntax()
            .children_with_tokens()
            .filter(|part| {
                !part.kind().is_trivia()
                    || matches!(
                        part.kind(),
                        SyntaxKind::INNER_DOC_COMMENT | SyntaxKind::OUTER_DOC_COMMENT
                    )
            })
            .collect();
        for (index, part) in parts.iter().enumerate() {
            if let Some(name) = identifier(part) {
                self.identifiers.insert(name.clone());
                if parts.get(index + 1).is_some_and(|part| group(part, true)) {
                    self.constructed.insert(name);
                }
            }
            if part.kind() == SyntaxKind::DOT {
                if let Some(name) = parts.get(index + 1).and_then(identifier) {
                    if !parts.get(index + 2).is_some_and(|part| group(part, false)) {
                        self.accessed_fields.insert(name);
                    }
                }
            }
            if let NodeOrToken::Node(node) = part {
                if let Some(tree) = ast::TokenTree::cast(node.clone()) {
                    self.visit(&tree);
                }
            }
            if matches!(
                part.kind(),
                SyntaxKind::INNER_DOC_COMMENT | SyntaxKind::OUTER_DOC_COMMENT
            ) {
                self.identifiers.insert("doc".to_string());
            }
        }
    }
}

#[instrument(level = "trace", skip(part))]
fn identifier(part: &SyntaxElement) -> Option<String> {
    (matches!(
        part.kind(),
        SyntaxKind::IDENT | SyntaxKind::LIFETIME_IDENT | SyntaxKind::UNDERSCORE
    ) || part.kind().is_keyword(ra_ap_syntax::Edition::Edition2024))
    .then(|| part.to_string().trim_start_matches('\'').to_string())
}

#[instrument(level = "trace", skip(part))]
fn group(part: &SyntaxElement, brace: bool) -> bool {
    part.as_node()
        .and_then(|node| ast::TokenTree::cast(node.clone()))
        .is_some_and(|tree| {
            if brace {
                tree.l_curly_token().is_some()
            } else {
                tree.l_paren_token().is_some()
            }
        })
}

#[instrument(level = "debug", skip(tree, invocation, syntax))]
pub(super) fn event(
    tree: Option<ast::TokenTree>,
    invocation: Option<&ast::MacroCall>,
    syntax: &DatabaseMacroSyntax,
) -> GraphExprEvent {
    let mut tokens = Tokens::default();
    if let Some(tree) = tree {
        tokens.visit(&tree);
    }
    let unhandled_fields = tokens
        .accessed_fields
        .iter()
        .filter(|field| invocation.is_none_or(|invocation| unhandled(invocation, field, syntax)))
        .cloned()
        .collect();
    GraphExprEvent::Macro {
        constructed_types: tokens.constructed,
        unhandled_fields,
        mentioned_types: tokens.identifiers,
    }
}

#[instrument(level = "debug", skip(invocation, field, syntax))]
fn unhandled(invocation: &ast::MacroCall, field: &str, syntax: &DatabaseMacroSyntax) -> bool {
    let mut tokens = Tokens::default();
    if let Some(tree) = invocation.token_tree() {
        tokens.visit(&tree);
    }
    if tokens.constructed.contains("Self") {
        return true;
    }
    if !tokens.accessed_fields.contains(field) {
        return false;
    }
    let args = syntax.arguments(invocation);
    if args.len() < 3
        || !matches!(args.get(1), Some(ast::Expr::Literal(lit)) if matches!(lit.kind(), ast::LiteralKind::String(_)))
    {
        return true;
    }
    args.iter().enumerate().any(|(index, arg)| {
        accesses(arg, field, syntax) && (index < 2 || !syntax::self_field(arg, field))
    })
}

#[instrument(level = "debug", skip(expr, field, syntax))]
fn accesses(expr: &ast::Expr, field: &str, syntax: &DatabaseMacroSyntax) -> bool {
    expr.syntax().descendants().any(|node| {
        if let Some(value) = ast::FieldExpr::cast(node.clone()) {
            if value.name_ref().is_some_and(|name| name.text() == field) {
                return true;
            }
        }
        if let Some(pattern) = ast::RecordPat::cast(node.clone()) {
            if pattern
                .path()
                .is_some_and(|path| syntax::names(path) == ["Self"])
                && pattern.record_pat_field_list().is_some_and(|list| {
                    list.fields().any(|value| {
                        value.name_ref().is_some_and(|name| name.text() == field)
                            || value.pat().is_some_and(|pat| match pat {
                                ast::Pat::IdentPat(pat) => {
                                    use ast::HasName;
                                    pat.name().is_some_and(|name| name.text() == field)
                                }
                                _ => false,
                            })
                    })
                })
            {
                return true;
            }
        }
        ast::MacroCall::cast(node).is_some_and(|invocation| unhandled(&invocation, field, syntax))
    })
}
