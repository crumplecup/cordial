//! A syntax snapshot issued only by the canonical source database.

use std::path::Path;

use ra_ap_syntax::{AstNode, SyntaxNode, TextRange, ast};
use sha2::{Digest, Sha256};

/// Source and tree are captured together, while the database is immutably
/// borrowed. Collectors cannot substitute a separately parsed tree.
#[derive(derive_getters::Getters)]
pub(crate) struct DatabaseSource<'db> {
    path: &'db Path,
    source: &'db str,
    tree: ast::SourceFile,
    line_index: &'db ra_ap_ide_db::line_index::LineIndex,
    #[getter(copy)]
    source_digest: [u8; 32],
}

impl<'db> DatabaseSource<'db> {
    #[tracing::instrument(level = "trace", skip(path, source, tree, line_index))]
    pub(super) fn new(
        path: &'db Path,
        source: &'db str,
        tree: ast::SourceFile,
        line_index: &'db ra_ap_ide_db::line_index::LineIndex,
    ) -> Self {
        Self {
            path,
            source,
            tree,
            line_index,
            source_digest: Sha256::digest(source.as_bytes()).into(),
        }
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
        self.line_index.line_col(offset).line + 1
    }
}
