//! Getter-consumer grammar islands, owned and memoized by the source loader.
use super::{DatabaseMacroSyntax, DatabaseSource, Payload, program};
use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode,
    ast::{self, HasModuleItem},
};

impl DatabaseSource<'_> {
    #[tracing::instrument(level = "debug", skip(self))]
    pub(crate) fn getter_macro_syntax(&self) -> DatabaseMacroSyntax {
        let mut syntax = DatabaseMacroSyntax::default();
        syntax.collect_getters(self, self.tree().syntax());
        syntax
    }

    #[tracing::instrument(level = "debug", skip(self, invocation))]
    fn getter_harness_items(&self, invocation: &ast::MacroCall) -> Option<(usize, Vec<ast::Item>)> {
        let tree = invocation.token_tree()?;
        let tokens: Vec<_> = tree
            .syntax()
            .children_with_tokens()
            .filter(|part| !part.kind().is_trivia())
            .collect();
        let [_, first, comma, second, comma2, body, _] = tokens.as_slice() else {
            return None;
        };
        if !(first.kind() == SyntaxKind::IDENT
            || first.kind().is_keyword(ra_ap_syntax::Edition::CURRENT))
            || !(second.kind() == SyntaxKind::IDENT
                || second.kind().is_keyword(ra_ap_syntax::Edition::CURRENT))
            || comma.kind() != SyntaxKind::COMMA
            || comma2.kind() != SyntaxKind::COMMA
        {
            return None;
        }
        let body = ast::TokenTree::cast(body.as_node()?.clone())?;
        body.l_curly_token()?;
        let key = Payload::new(self.database(), self.proof_body_text(&body));
        let parsed = program(self.database(), key);
        if !parsed.errors().is_empty() {
            return None;
        }
        Some((
            usize::from(body.syntax().text_range().start()) + 1,
            parsed.tree().items().collect(),
        ))
    }
}

impl DatabaseMacroSyntax {
    #[tracing::instrument(level = "debug", skip(self, file, root))]
    fn collect_getters(&mut self, file: &DatabaseSource<'_>, root: &SyntaxNode) {
        for invocation in root.descendants().filter_map(ast::MacroCall::cast) {
            let names = invocation
                .path()
                .map(|path| {
                    path.syntax()
                        .descendants()
                        .filter_map(ast::NameRef::cast)
                        .map(|name| name.text().to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if names.last().is_some_and(|name| name == "format")
                || matches!(names.as_slice(), [name] if expression_arguments(name))
                || matches!(names.as_slice(), [root, name] if matches!(root.as_str(), "std" | "core") && expression_arguments(name))
            {
                if let Some(arguments) = file.try_macro_arguments(&invocation) {
                    for argument in &arguments {
                        self.collect_getters(file, argument.syntax());
                    }
                    if let Some(tree) = invocation.token_tree() {
                        self.getter_offsets.insert(
                            invocation.clone(),
                            usize::from(tree.syntax().text_range().start()),
                        );
                        self.arguments.insert(invocation, arguments);
                    }
                }
            } else if matches!(names.as_slice(), [root, name] if root == "amenable_derive" && matches!(name.as_str(), "harness" | "gallery_harness"))
            {
                if let Some((offset, items)) = file.getter_harness_items(&invocation) {
                    for item in &items {
                        self.collect_getters(file, item.syntax());
                    }
                    self.getter_offsets.insert(invocation.clone(), offset);
                    self.items.insert(invocation, items);
                }
            }
        }
    }
}

#[tracing::instrument(level = "trace")]
fn expression_arguments(name: &str) -> bool {
    matches!(
        name,
        "assert"
            | "assert_eq"
            | "assert_ne"
            | "debug_assert"
            | "debug_assert_eq"
            | "debug_assert_ne"
            | "vec"
            | "format"
            | "format_args"
            | "write"
            | "writeln"
    )
}
