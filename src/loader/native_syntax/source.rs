//! A syntax snapshot issued only by the canonical source database.

use std::path::Path;

use ra_ap_syntax::{AstNode, SyntaxNode, TextRange, ast};
use sha2::{Digest, Sha256};

/// Source and tree are captured together, while the database is immutably
/// borrowed. Collectors cannot substitute a separately parsed tree.
#[derive(derive_getters::Getters)]
pub(crate) struct DatabaseSource<'db> {
    #[cfg(any(
        feature = "tracing",
        feature = "internal_error_chain",
        feature = "verus_ir",
        feature = "panics",
        feature = "antipatterns"
    ))]
    database: &'db ra_ap_ide_db::RootDatabase,
    path: &'db Path,
    source: &'db str,
    tree: ast::SourceFile,
    line_index: &'db ra_ap_ide_db::line_index::LineIndex,
    #[getter(skip)]
    line_base: u32,
    #[getter(copy)]
    source_digest: [u8; 32],
}

impl<'db> DatabaseSource<'db> {
    #[tracing::instrument(level = "trace", skip_all)]
    pub(in crate::loader) fn new(
        #[cfg(any(
            feature = "tracing",
            feature = "internal_error_chain",
            feature = "verus_ir",
            feature = "panics",
            feature = "antipatterns"
        ))]
        database: &'db ra_ap_ide_db::RootDatabase,
        path: &'db Path,
        source: &'db str,
        tree: ast::SourceFile,
        line_index: &'db ra_ap_ide_db::line_index::LineIndex,
    ) -> Self {
        Self {
            #[cfg(any(
                feature = "tracing",
                feature = "internal_error_chain",
                feature = "verus_ir",
                feature = "panics",
                feature = "antipatterns"
            ))]
            database,
            path,
            source,
            tree,
            line_index,
            line_base: 0,
            source_digest: Sha256::digest(source.as_bytes()).into(),
        }
    }

    /// Locate an issued grammar island in its enclosing canonical source.
    #[cfg(feature = "antipatterns")]
    #[tracing::instrument(level = "trace", skip_all)]
    pub(in crate::loader) fn locate_macro_body(
        mut self,
        parent: &DatabaseSource<'_>,
        body: &ast::TokenTree,
    ) -> Option<Self> {
        if body.syntax().ancestors().last().as_ref() != Some(parent.tree().syntax()) {
            return None;
        }
        let text = parent.text(body.syntax());
        if text.get(1..text.len().saturating_sub(1))? != self.source {
            return None;
        }
        self.line_base = parent
            .line(body.syntax().text_range().start())
            .saturating_sub(1);
        Some(self)
    }

    /// Select a function from this snapshot, rather than accepting foreign AST.
    #[tracing::instrument(level = "trace", skip(self, range))]
    pub(crate) fn function(&self, range: TextRange) -> Option<ast::Fn> {
        self.tree
            .syntax()
            .covering_element(range)
            .into_node()
            .filter(|node| node.text_range() == range)
            .and_then(ast::Fn::cast)
    }

    #[tracing::instrument(level = "trace", skip(self, node))]
    pub(crate) fn text(&self, node: &SyntaxNode) -> &str {
        &self.source[std::ops::Range::<usize>::from(node.text_range())]
    }

    #[tracing::instrument(level = "trace", skip(self, offset))]
    pub(crate) fn line(&self, offset: ra_ap_syntax::TextSize) -> u32 {
        self.line_index.line_col(offset).line + 1 + self.line_base
    }
}
