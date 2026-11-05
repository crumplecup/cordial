//! Loader-issued grammar islands in canonical macro payloads.

use super::{DatabaseSource, Payload, expression};
use ra_ap_syntax::{AstNode, ast};

impl DatabaseSource<'_> {
    #[tracing::instrument(level = "debug", skip(self, invocation))]
    pub(in crate::loader::native_syntax) fn macro_arguments(
        &self,
        invocation: &ast::MacroCall,
    ) -> Vec<ast::Expr> {
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
    #[cfg(any(
        feature = "tracing",
        feature = "internal_error_chain",
        feature = "antipatterns"
    ))]
    #[tracing::instrument(level = "debug", skip(self, invocation))]
    pub(in crate::loader::native_syntax) fn macro_items(
        &self,
        invocation: &ast::MacroCall,
    ) -> Vec<ast::Item> {
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
        let text = self.proof_body_text(&body);
        let key = Payload::new(self.database(), text);
        let parsed = super::program(self.database(), key);
        if !parsed.errors().is_empty() {
            return Vec::new();
        }
        use ast::HasModuleItem;
        parsed.tree().items().collect()
    }

    #[cfg(any(
        feature = "tracing",
        feature = "internal_error_chain",
        feature = "antipatterns"
    ))]
    #[tracing::instrument(level = "trace", skip(self, body))]
    pub(in crate::loader::native_syntax) fn proof_body_text(
        &self,
        body: &ast::TokenTree,
    ) -> String {
        let text = body.syntax().text().to_string();
        text[1..text.len() - 1].to_string()
    }
}
