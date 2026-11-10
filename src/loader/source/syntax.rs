//! The read-only syntax handle issued to source enrichers and plugins.

use std::path::Path;

use ra_ap_syntax::{TextSize, ast};

use crate::FileSnapshot;
use crate::loader::DatabaseSource;

/// One source file's syntax tree, issued by the run's database.
///
/// Text, path and tree are captured together, so a plugin cannot pair a tree
/// from its own parse with another file's identity. Construction is private to
/// cordial. The tree is rust-analyzer's, at the version re-exported as
/// [`crate::syntax`]; reach it with [`syntax`](Self::syntax) or
/// [`into_inner`](Self::into_inner) and import its types from that re-export so
/// plugin and host agree on one version.
///
/// Facts derived from the tree should be stored with
/// [`snapshot`](Self::snapshot), as the built-in inventories do, so they can be
/// replayed after the source changes.
#[derive(derive_new::new)]
#[new(visibility = "pub(crate)")]
pub struct SourceSyntax<'a> {
    inner: &'a DatabaseSource<'a>,
}

impl<'a> SourceSyntax<'a> {
    /// Path the database loaded this file from.
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn path(&self) -> &Path {
        self.inner.path()
    }

    /// The exact source text the tree was parsed from.
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn text(&self) -> &str {
        self.inner.source()
    }

    /// Path and digest of the text this tree was parsed from.
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn snapshot(&self) -> FileSnapshot {
        FileSnapshot::new(self.path().to_path_buf(), self.inner.source_digest())
    }

    /// One-based line of a byte offset into [`text`](Self::text).
    #[tracing::instrument(level = "trace", skip(self, offset))]
    pub fn line(&self, offset: TextSize) -> u32 {
        self.inner.line(offset)
    }

    /// The parsed file, borrowed.
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn syntax(&self) -> &ast::SourceFile {
        self.inner.tree()
    }

    /// The parsed file, consumed.
    ///
    /// The tree is an immutable snapshot and may be kept past the callback that
    /// issued this handle. It is then no longer tied to the database, so it may
    /// be stale against a later revision.
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn into_inner(self) -> ast::SourceFile {
        self.inner.tree().clone()
    }
}
