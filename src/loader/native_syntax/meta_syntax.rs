//! Attribute argument grammar uses canonical tokens and memoized value syntax.
use super::{
    DatabaseSource,
    macro_syntax::{Payload, expression},
};
use ra_ap_syntax::{AstNode, NodeOrToken, SyntaxElement, SyntaxKind, ast};

impl DatabaseSource<'_> {
    /// Expression arguments of an attribute issued by this canonical file.
    #[tracing::instrument(level = "trace", skip(self, meta))]
    pub(crate) fn meta_expressions(&self, meta: &ast::TokenTreeMeta) -> Option<Vec<ast::Expr>> {
        if meta.syntax().ancestors().last().as_ref() != Some(self.tree().syntax()) {
            return None;
        }
        let tree = meta.token_tree()?;
        let text = tree.syntax().text().to_string();
        let inner = text.get(1..text.len().checked_sub(1)?)?;
        let key = Payload::new(self.database(), format!("({inner})"));
        let parsed = expression(self.database(), key);
        if !parsed.errors().is_empty() {
            return None;
        }
        match ast::Expr::cast(parsed.syntax_node())? {
            ast::Expr::TupleExpr(tuple) => Some(tuple.fields().collect()),
            ast::Expr::ParenExpr(paren) => Some(paren.expr().into_iter().collect()),
            _ => None,
        }
    }

    #[tracing::instrument(level = "trace", skip(self, attr))]
    pub(crate) fn meta_has_argument(&self, attr: &ast::Attr, wanted: &str) -> bool {
        if attr.syntax().ancestors().last().as_ref() != Some(self.tree().syntax()) {
            return false;
        }
        let Some(ast::Meta::TokenTreeMeta(meta)) = attr.meta() else {
            return false;
        };
        let Some(tree) = meta.token_tree() else {
            return false;
        };
        let tokens: Vec<_> = tree
            .syntax()
            .children_with_tokens()
            .filter(|t| !t.kind().is_trivia())
            .collect();
        let [_, inner @ .., _] = tokens.as_slice() else {
            return false;
        };
        let mut found = false;
        let mut offset = 0;
        while offset < inner.len() {
            let Some((path, end)) = meta_path(&inner[offset..]) else {
                return false;
            };
            offset += end;
            match inner.get(offset) {
                Some(NodeOrToken::Node(node)) if ast::TokenTree::can_cast(node.kind()) => {
                    offset += 1;
                }
                Some(eq) if eq.kind() == SyntaxKind::EQ => {
                    let Some(end) = self.meta_value_end(&inner[offset + 1..]) else {
                        return false;
                    };
                    offset += 1 + end;
                }
                _ => {}
            }
            found |= path == wanted;
            if let Some(next) = inner.get(offset) {
                if next.kind() != SyntaxKind::COMMA {
                    return false;
                }
                offset += 1;
            }
        }
        found
    }

    /// Commas inside closures and generic arguments belong to the value, not
    /// the enclosing meta list. Ask the shared expression grammar to identify
    /// a complete value at each candidate delimiter.
    #[tracing::instrument(level = "trace", skip(self, tokens))]
    fn meta_value_end(&self, tokens: &[SyntaxElement]) -> Option<usize> {
        let first = tokens.first()?;
        (1..=tokens.len()).find(|end| {
            if tokens
                .get(*end)
                .is_some_and(|next| next.kind() != SyntaxKind::COMMA)
            {
                return false;
            }
            let range = ra_ap_syntax::TextRange::new(
                first.text_range().start(),
                tokens[end - 1].text_range().end(),
            );
            let text = &self.source()[std::ops::Range::<usize>::from(range)];
            let key = Payload::new(self.database(), text.to_owned());
            expression(self.database(), key).errors().is_empty()
        })
    }
}
#[tracing::instrument(level = "trace", skip(tokens))]
fn meta_path(tokens: &[SyntaxElement]) -> Option<(String, usize)> {
    let mut end = 0;
    let mut path = String::new();
    if tokens
        .first()
        .is_some_and(|t| t.kind() == SyntaxKind::COLON2)
    {
        path.push_str("::");
        end += 1;
    }
    loop {
        let token = tokens.get(end)?.as_token()?;
        if token.kind() != SyntaxKind::IDENT
            && !matches!(token.text(), "self" | "Self" | "super" | "crate")
        {
            return None;
        }
        path.push_str(token.text());
        end += 1;
        if tokens
            .get(end)
            .is_some_and(|t| t.kind() == SyntaxKind::COLON2)
        {
            path.push_str("::");
            end += 1;
        } else {
            break;
        }
    }
    Some((path, end))
}
