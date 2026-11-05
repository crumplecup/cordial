//! Canonical macro islands issued to proof and error-apply collectors.

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

#[cfg(feature = "internal_error_chain")]
impl DatabaseMacroSyntax {
    #[tracing::instrument(level = "debug", skip(self, file, root))]
    fn collect_error_apply(&mut self, file: &DatabaseSource<'_>, root: &ra_ap_syntax::SyntaxNode) {
        for invocation in root.descendants().filter_map(ast::MacroCall::cast) {
            let is_write = invocation.path().is_some_and(|path| {
                path.qualifier().is_none()
                    && path.coloncolon_token().is_none()
                    && path
                        .segment()
                        .and_then(|segment| segment.name_ref())
                        .is_some_and(|name| matches!(name.text(), "write" | "writeln"))
            });
            if is_write {
                let arguments = file.macro_arguments(&invocation);
                for arg in &arguments {
                    self.collect_error_apply(file, arg.syntax());
                }
                self.arguments.insert(invocation, arguments);
            }
        }
    }
}

#[cfg(feature = "internal_error_chain")]
impl DatabaseSource<'_> {
    /// Formatting arguments issued only from this snapshot's canonical syntax.
    #[tracing::instrument(level = "debug", skip(self))]
    pub(crate) fn error_apply_macro_syntax(&self) -> DatabaseMacroSyntax {
        let mut syntax = DatabaseMacroSyntax::default();
        syntax.collect_error_apply(self, self.tree().syntax());
        syntax
    }
}
