//! Ordinary identifiers and opaque macro token mentions.

use super::{ApplyUses, located, opaque_meta};
use crate::loader::DatabaseSource;
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxToken, TextSize, ast};

impl ApplyUses {
    #[tracing::instrument(level = "trace", skip(self, file))]
    pub(super) fn collect_tokens(&mut self, file: &DatabaseSource<'_>) {
        for token in file
            .tree()
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
        {
            if token
                .parent_ancestors()
                .any(|node| opaque_meta(node.kind()))
            {
                continue;
            }
            let in_tokens = token
                .parent_ancestors()
                .any(|node| node.kind() == SyntaxKind::TOKEN_TREE);
            if in_tokens {
                if !token.parent_ancestors().any(|node| {
                    matches!(
                        node.kind(),
                        SyntaxKind::MACRO_CALL | SyntaxKind::MACRO_RULES
                    )
                }) {
                    continue;
                }
                if token.kind() == SyntaxKind::IDENT
                    || token.kind() == SyntaxKind::LIFETIME_IDENT
                    || token.kind() == SyntaxKind::UNDERSCORE
                    || token.kind().is_keyword(ra_ap_syntax::Edition::Edition2024)
                {
                    self.macro_names
                        .insert(token.text().trim_start_matches('\'').to_string());
                }
                if matches!(
                    token.kind(),
                    SyntaxKind::INNER_DOC_COMMENT | SyntaxKind::OUTER_DOC_COMMENT
                ) {
                    self.macro_names.insert("doc".to_string());
                }
            } else if ordinary_ident(&token) {
                if token.kind() == SyntaxKind::SELF_KW
                    && token
                        .parent()
                        .and_then(|node| node.parent())
                        .is_some_and(|node| ast::SelfParam::can_cast(node.kind()))
                {
                    continue;
                }
                let start = token.text_range().start()
                    + TextSize::from(u32::from(token.kind() == SyntaxKind::LIFETIME_IDENT));
                self.identifiers.insert(located(
                    file,
                    token.text().trim_start_matches('\'').to_string(),
                    start,
                ));
            } else if matches!(
                token.kind(),
                SyntaxKind::MACRO_RULES_KW
                    | SyntaxKind::INNER_DOC_COMMENT
                    | SyntaxKind::OUTER_DOC_COMMENT
            ) {
                let label = if token.kind() == SyntaxKind::MACRO_RULES_KW {
                    "macro_rules"
                } else {
                    "doc"
                };
                let name = located(file, label.to_string(), token.text_range().start());
                self.identifiers.insert(name.clone());
                if !token
                    .parent_ancestors()
                    .any(|node| ast::Module::can_cast(node.kind()))
                {
                    self.paths.push((name, vec![label.to_string()]));
                }
            }
        }
    }
}

#[tracing::instrument(level = "trace", skip(token))]
fn ordinary_ident(token: &SyntaxToken) -> bool {
    token.kind() == SyntaxKind::LIFETIME_IDENT
        || token.parent().is_some_and(|node| {
            ast::Name::can_cast(node.kind()) || ast::NameRef::can_cast(node.kind())
        })
        || (token.kind() == SyntaxKind::UNDERSCORE
            && token
                .parent()
                .is_some_and(|node| ast::Rename::can_cast(node.kind())))
}
