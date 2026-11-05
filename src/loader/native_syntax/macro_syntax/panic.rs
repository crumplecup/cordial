//! Panic proof islands and embedded programs issued by the source database.
use super::{DatabaseMacroSyntax, DatabaseSource, Payload, program};
use ra_ap_syntax::{AstNode, ast, ast::HasModuleItem};

impl DatabaseSource<'_> {
    /// Preserve the panic policy's broader macro grammar without changing the
    /// assertion-only call policy used by tracing. Every parse is memoized here.
    #[tracing::instrument(level = "debug", skip(self))]
    pub(crate) fn panic_macro_syntax(&self) -> DatabaseMacroSyntax {
        let mut syntax = DatabaseMacroSyntax::default();
        syntax.collect_panics(self, self.tree().syntax());
        syntax
    }

    /// The literal must belong to this carrier. An embedded program receives
    /// its own captured text, canonical payload tree and line index together.
    #[tracing::instrument(level = "trace", skip(self, literal, collect))]
    pub(crate) fn with_embedded_program<T>(
        &self,
        literal: &ast::Literal,
        collect: impl FnOnce(&DatabaseSource<'_>) -> T,
    ) -> Option<T> {
        if literal.syntax().ancestors().last().as_ref() != Some(self.tree().syntax()) {
            return None;
        }
        let ast::LiteralKind::String(token) = literal.kind() else {
            return None;
        };
        let value = token.value().ok()?;
        if ![
            "panic!",
            "unreachable!",
            "compile_error!",
            ".unwrap(",
            ".unwrap_err(",
            ".expect(",
            ".expect_err(",
        ]
        .iter()
        .any(|needle| value.contains(needle))
        {
            return None;
        }
        let key = Payload::new(self.database(), value.to_string());
        let parsed = program(self.database(), key);
        if !parsed.errors().is_empty() {
            return None;
        }
        let lines = ra_ap_ide_db::line_index::LineIndex::new(&value);
        let source =
            DatabaseSource::new(self.database(), self.path(), &value, parsed.tree(), &lines);
        Some(collect(&source))
    }

    /// Without the optional Verus grammar, preserve best-effort function-body
    /// recovery. The database owns each recovered Rust grammar island too.
    #[cfg(not(feature = "verus_ir"))]
    #[tracing::instrument(level = "trace", skip(self, invocation, collect))]
    pub(crate) fn with_recovered_verus(
        &self,
        invocation: &ast::MacroCall,
        mut collect: impl FnMut(&DatabaseSource<'_>) -> crate::CordialResult<()>,
    ) -> crate::CordialResult<()> {
        if invocation.syntax().ancestors().last().as_ref() != Some(self.tree().syntax()) {
            return Ok(());
        }
        if let Some(tree) = invocation.token_tree() {
            self.recover_functions(&tree, &mut collect)?;
        }
        Ok(())
    }

    #[cfg(not(feature = "verus_ir"))]
    #[tracing::instrument(level = "debug", skip(self, tree, collect), err(level = "warn"))]
    fn recover_functions(
        &self,
        tree: &ast::TokenTree,
        collect: &mut impl FnMut(&DatabaseSource<'_>) -> crate::CordialResult<()>,
    ) -> crate::CordialResult<()> {
        let mut parts = tree
            .syntax()
            .children_with_tokens()
            .filter(|part| !part.kind().is_trivia())
            .peekable();
        while let Some(part) = parts.next() {
            if part.as_token().is_some_and(|token| token.text() == "fn") {
                let Some(name) = parts.next().and_then(|part| part.into_token()) else {
                    continue;
                };
                let body = parts
                    .by_ref()
                    .filter_map(|part| part.into_node())
                    .filter_map(ast::TokenTree::cast)
                    .find(|tree| tree.l_curly_token().is_some());
                let Some(body) = body else { continue };
                let line = self.line(body.syntax().text_range().start());
                let text = format!(
                    "{}fn {}() {}",
                    "\n".repeat(line.saturating_sub(1) as usize),
                    name.text(),
                    body.syntax().text()
                );
                let key = Payload::new(self.database(), text.clone());
                let parsed = program(self.database(), key);
                if parsed.errors().is_empty() {
                    let lines = ra_ap_ide_db::line_index::LineIndex::new(&text);
                    let source = DatabaseSource::new(
                        self.database(),
                        self.path(),
                        &text,
                        parsed.tree(),
                        &lines,
                    );
                    collect(&source)?;
                } else {
                    self.recover_functions(&body, collect)?;
                }
            } else if let Some(tree) = part.into_node().and_then(ast::TokenTree::cast) {
                self.recover_functions(&tree, collect)?;
            }
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip(self, invocation))]
    fn panic_macro_items(&self, invocation: &ast::MacroCall) -> Vec<ast::Item> {
        let Some(tree) = invocation.token_tree() else {
            return Vec::new();
        };
        let text = tree.syntax().text().to_string();
        let key = Payload::new(self.database(), text[1..text.len() - 1].to_string());
        let whole = program(self.database(), key);
        if whole.errors().is_empty() {
            return whole.tree().items().collect();
        }
        let Some(body) = tree
            .syntax()
            .children()
            .filter_map(ast::TokenTree::cast)
            .filter(|tree| tree.l_curly_token().is_some())
            .last()
        else {
            return Vec::new();
        };
        let text = body.syntax().text().to_string();
        let key = Payload::new(self.database(), text[1..text.len() - 1].to_string());
        let parsed = program(self.database(), key);
        let items: Vec<_> = parsed.tree().items().collect();
        if parsed.errors().is_empty() && items.len() == 1 {
            items
        } else {
            Vec::new()
        }
    }
}

impl DatabaseMacroSyntax {
    #[tracing::instrument(level = "debug", skip(self, file, root))]
    fn collect_panics(&mut self, file: &DatabaseSource<'_>, root: &ra_ap_syntax::SyntaxNode) {
        for invocation in root.descendants().filter_map(ast::MacroCall::cast) {
            let items = file.panic_macro_items(&invocation);
            for item in &items {
                self.collect_panics(file, item.syntax());
            }
            self.items.insert(invocation.clone(), items);
            let is_expression = invocation.syntax().parent().is_some_and(|parent| {
                ast::MacroExpr::can_cast(parent.kind()) || ast::StmtList::can_cast(parent.kind())
            });
            if is_expression {
                let arguments = file.macro_arguments(&invocation);
                for arg in &arguments {
                    self.collect_panics(file, arg.syntax());
                }
                self.arguments.insert(invocation, arguments);
            }
        }
    }
}
