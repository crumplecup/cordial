//! Consume loader-issued grammar islands; unknown expansion contracts stay opaque.
use super::{Collector, ReceiverRecipe, UseKind, contracts};
use ra_ap_syntax::{
    AstNode, SyntaxKind,
    ast::{self, HasModuleItem, HasName},
};

impl Collector<'_, '_> {
    #[tracing::instrument(level = "trace", skip(self, invocation))]
    pub(super) fn invocation(&mut self, invocation: &ast::MacroCall, offset: usize) {
        if !self.scope.local_types && macro_contract(self.file, invocation) {
            if let Some((origin, arguments, items)) = self.syntax.getter_payload(invocation) {
                let arguments = arguments.to_vec();
                let items = items.to_vec();
                if let (Some(path), Some(tree)) = (invocation.path(), invocation.token_tree()) {
                    self.record(
                        offset + usize::from(path.syntax().text_range().start()),
                        offset + usize::from(tree.syntax().text_range().end()),
                        ReceiverRecipe::Unknown,
                        UseKind::Macro,
                    );
                }
                for argument in arguments {
                    self.visit(argument.syntax(), offset + origin);
                }
                for item in items {
                    self.visit(item.syntax(), offset + origin);
                }
                return;
            }
        }
        if let Some(tree) = invocation.token_tree() {
            self.opaque(&tree, offset);
        }
    }

    #[tracing::instrument(level = "trace", skip(self, tree))]
    pub(super) fn opaque(&mut self, tree: &ast::TokenTree, offset: usize) {
        let tokens: Vec<_> = tree
            .syntax()
            .children_with_tokens()
            .filter(|token| !token.kind().is_trivia())
            .collect();
        for (index, token) in tokens.iter().enumerate() {
            if let Some(group) = token
                .as_node()
                .and_then(|node| ast::TokenTree::cast(node.clone()))
            {
                self.opaque(&group, offset);
            } else if token.as_token().is_some_and(|token| token.text() == "file")
                && index > 0
                && matches!(
                    tokens[index - 1].kind(),
                    SyntaxKind::DOT | SyntaxKind::COLON | SyntaxKind::COLON2
                )
            {
                let range = token.text_range();
                self.record(
                    offset + usize::from(range.start()),
                    offset + usize::from(range.end()),
                    ReceiverRecipe::Unknown,
                    UseKind::Getter,
                );
            }
        }
    }
}

#[tracing::instrument(level = "trace", skip(file, invocation))]
fn macro_contract(file: &crate::loader::DatabaseSource<'_>, invocation: &ast::MacroCall) -> bool {
    let Some((path, _)) = invocation
        .path()
        .and_then(|path| contracts::path_parts(&path))
    else {
        return false;
    };
    let Some(name) = path.last() else {
        return false;
    };
    if file.tree().items().any(|item| matches!(item, ast::Item::MacroRules(item) if item.name().is_some_and(|ident| ident.text() == name))) { return false; }
    if matches!(name.as_str(), "harness" | "gallery_harness") {
        return super::super::super::getters::known_native_derive(
            file,
            &path,
            "amenable_derive",
            name,
        );
    }
    (path.len() == 1 && contracts::unshadowed(file, name))
        || super::super::super::getters::known_native_derive(file, &path, "std", name)
        || super::super::super::getters::known_native_derive(file, &path, "core", name)
}
