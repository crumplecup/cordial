//! Attribute facts, without reparsing token trees.
use ra_ap_syntax::{
    AstNode, AstToken,
    ast::{self, HasAttrs},
};

#[tracing::instrument(level = "trace", skip(owner))]
pub(super) fn gate(owner: &impl HasAttrs, name: &str) -> bool {
    owner.attrs().any(|attr| match attr.meta() {
        Some(ast::Meta::CfgMeta(meta)) => meta.cfg_predicate().is_some_and(|predicate| {
            super::super::tokens::render(predicate.syntax()).replace(' ', "") == name
        }),
        _ => false,
    })
}

#[tracing::instrument(level = "trace", skip(owner))]
pub(super) fn has_derive(owner: &impl HasAttrs, needle: &str) -> bool {
    owner.attrs().any(|attr| {
        if attr.simple_name().as_deref() != Some("derive") {
            return false;
        }
        let Some(ast::Meta::TokenTreeMeta(meta)) = attr.meta() else {
            return false;
        };
        let Some(tree) = meta.token_tree() else {
            return false;
        };
        let text = super::super::tokens::render(tree.syntax());
        let text = text.trim_start_matches('(').trim_end_matches(')');
        text.split(',').any(|part| {
            let compact = part.replace(' ', "");
            compact == needle
                || compact.ends_with(&format!("::{needle}"))
                || compact.contains(&format!("::{needle}::"))
        })
    })
}

#[tracing::instrument(level = "trace", skip(owner))]
pub(super) fn builder_names(owner: &impl HasAttrs, name: &str) -> Vec<String> {
    if !has_derive(owner, "Builder") {
        return Vec::new();
    }
    let mut names = vec![format!("{name}Builder")];
    for attr in owner
        .attrs()
        .filter(|attr| attr.simple_name().as_deref() == Some("builder"))
    {
        let Some(ast::Meta::TokenTreeMeta(meta)) = attr.meta() else {
            continue;
        };
        let Some(tree) = meta.token_tree() else {
            continue;
        };
        // Only direct `name = STRING` arguments; nested options have their own trees.
        let tokens: Vec<_> = tree
            .syntax()
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| !token.kind().is_trivia())
            .collect();
        let [_, contents @ .., _] = tokens.as_slice() else {
            continue;
        };
        for segment in contents.split(|token| token.text() == ",") {
            if let [key, eq, value] = segment
                && key.text() == "name"
                && eq.text() == "="
                && let Some(value) = ast::String::cast(value.clone())
                && let Ok(value) = value.value()
            {
                names.push(value.into_owned());
                break;
            }
        }
    }
    names.sort();
    names.dedup();
    names
}
