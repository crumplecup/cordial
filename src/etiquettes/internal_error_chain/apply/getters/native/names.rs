//! Identifier counts include opaque attributes and macro token trees.
use crate::etiquettes::internal_error_chain::apply::getters::names::Names;
use crate::loader::DatabaseSource;
use ra_ap_syntax::{AstNode, SyntaxKind};

#[tracing::instrument(level = "debug", skip(file))]
pub(super) fn collect(file: &DatabaseSource<'_>) -> Names {
    let mut names = Names::default();
    for token in file
        .tree()
        .syntax()
        .descendants_with_tokens()
        .filter_map(|part| part.into_token())
    {
        let text = token.text();
        if token.kind().is_any_identifier() || token.kind() == SyntaxKind::UNDERSCORE {
            names.record(text);
        } else if token.kind() == SyntaxKind::LIFETIME_IDENT {
            names.record(text.trim_start_matches('\''));
        } else if matches!(
            token.kind(),
            SyntaxKind::OUTER_DOC_COMMENT | SyntaxKind::INNER_DOC_COMMENT
        ) {
            names.record("doc");
        }
    }
    names
}
