//! Canonical macro forest shared by proof-aware collectors.

use super::{DatabaseMacroSyntax, DatabaseSource};
use ra_ap_syntax::{AstNode, ast};

impl DatabaseMacroSyntax {
    #[tracing::instrument(level = "debug", skip(self, file, root))]
    fn collect(&mut self, file: &DatabaseSource<'_>, root: &ra_ap_syntax::SyntaxNode) {
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
            ) || name
                .as_deref()
                .is_some_and(|name| name.rsplit("::").next() == Some("format"))
            {
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
}
