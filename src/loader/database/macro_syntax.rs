//! Memoized grammar projections for Rust islands in opaque macro token trees.
//!
//! File parsing leaves token trees opaque. These queries parse each supported
//! payload once, with ownership in the source database, rather than in a lint.
//! Recognizing Rust syntax does not establish a macro's expansion semantics.

use super::{DatabaseSource, queries::SemanticDatabase};
use ra_ap_syntax::{AstNode, Parse, SyntaxNode, ast};
use std::collections::HashMap;

#[salsa::interned]
struct Payload<'db> {
    text: String,
}

#[salsa::tracked(returns(ref))]
#[tracing::instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
fn expression<'db>(db: &'db dyn SemanticDatabase, key: Payload<'db>) -> Parse<ast::Expr> {
    ast::Expr::parse(key.text(db), ra_ap_syntax::Edition::CURRENT)
}

/// Loader-issued syntax only. Lookups use canonical AST identity; a foreign
/// parse cannot add evidence or cause another payload to be parsed.
#[derive(Default)]
pub(crate) struct DatabaseMacroSyntax {
    arguments: HashMap<ast::MacroCall, Vec<ast::Expr>>,
    items: HashMap<ast::MacroCall, Vec<ast::Item>>,
}

impl DatabaseMacroSyntax {
    #[tracing::instrument(level = "trace", skip(self, invocation))]
    pub(crate) fn arguments(&self, invocation: &ast::MacroCall) -> &[ast::Expr] {
        self.arguments
            .get(invocation)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    #[tracing::instrument(level = "trace", skip(self, invocation))]
    pub(crate) fn items(&self, invocation: &ast::MacroCall) -> &[ast::Item] {
        self.items
            .get(invocation)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    #[tracing::instrument(level = "debug", skip(self, file, root))]
    fn collect(&mut self, file: &DatabaseSource<'_>, root: &SyntaxNode) {
        for invocation in root.descendants().filter_map(ast::MacroCall::cast) {
            let name = invocation
                .path()
                .map(|path| path.syntax().text().to_string());
            if matches!(
                name.as_deref(),
                Some(
                    "assert"
                        | "debug_assert"
                        | "assert_eq"
                        | "assert_ne"
                        | "debug_assert_eq"
                        | "debug_assert_ne"
                )
            ) {
                let arguments = file.macro_arguments(&invocation);
                for arg in &arguments {
                    self.collect(file, arg.syntax());
                }
                self.arguments.insert(invocation, arguments);
            } else {
                let items = file.macro_items(&invocation);
                for item in &items {
                    self.collect(file, item.syntax());
                }
                self.items.insert(invocation, items);
            }
        }
    }
}

impl DatabaseSource<'_> {
    /// Project opaque islands only from this snapshot and its issued islands.
    #[tracing::instrument(level = "debug", skip(self))]
    pub(crate) fn proof_macro_syntax(&self) -> DatabaseMacroSyntax {
        let mut syntax = DatabaseMacroSyntax::default();
        syntax.collect(self, self.tree().syntax());
        syntax
    }

    #[tracing::instrument(level = "debug", skip(self, invocation))]
    fn macro_arguments(&self, invocation: &ast::MacroCall) -> Vec<ast::Expr> {
        let Some(tree) = invocation.token_tree() else {
            return Vec::new();
        };
        let text = tree.syntax().text().to_string();
        let Some(inner) = text.get(1..text.len().saturating_sub(1)) else {
            return Vec::new();
        };
        let key = Payload::new(self.database(), format!("({inner})"));
        let parsed = expression(self.database(), key);
        if !parsed.errors().is_empty() {
            return Vec::new();
        }
        match ast::Expr::cast(parsed.syntax_node()) {
            Some(ast::Expr::TupleExpr(tuple)) => tuple.fields().collect(),
            Some(ast::Expr::ParenExpr(paren)) => paren.expr().into_iter().collect(),
            _ => Vec::new(),
        }
    }

    /// Items from the last top-level braced argument, matching the workspace
    /// proof-harness syntax contract. Unknown/malformed payloads remain opaque.
    #[tracing::instrument(level = "debug", skip(self, invocation))]
    fn macro_items(&self, invocation: &ast::MacroCall) -> Vec<ast::Item> {
        let Some(body) = invocation.token_tree().and_then(|tree| {
            tree.syntax()
                .children_with_tokens()
                .filter_map(|part| match part {
                    ra_ap_syntax::NodeOrToken::Node(node) => {
                        ast::TokenTree::cast(node).filter(|tree| tree.l_curly_token().is_some())
                    }
                    _ => None,
                })
                .last()
        }) else {
            return Vec::new();
        };
        let key = Payload::new(self.database(), body.syntax().text().to_string());
        let parsed = expression(self.database(), key);
        if !parsed.errors().is_empty() {
            return Vec::new();
        }
        ast::BlockExpr::cast(parsed.syntax_node())
            .and_then(|block| block.stmt_list())
            .map(|list| {
                list.statements()
                    .filter_map(|stmt| match stmt {
                        ast::Stmt::Item(item) => Some(item),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}
