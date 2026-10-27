//! Source-only view and snapshot identity for error graph apply.

use std::path::Path;
use std::rc::Rc;

use tracing::instrument;

use crate::error::CordialResult;

use super::{ParsedSource, SharedParsedFile, matches_snapshot};

/// Snapshot identity shared by collection and source-only planning views.
pub(crate) trait SourceSnapshot {
    fn path(&self) -> &Path;
    fn source_digest(&self) -> [u8; 32];
}

/// Source snapshot available to edit planning without syntax-tree access.
#[derive(Debug, Clone)]
pub(crate) struct SourceEditView {
    inner: Rc<ParsedSource>,
}

impl SharedParsedFile {
    /// Issue the text-only view used after collection has finished.
    #[instrument(level = "trace", skip(self))]
    pub(crate) fn edit_view(&self) -> SourceEditView {
        SourceEditView {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl SourceEditView {
    #[instrument(level = "trace", skip(self))]
    pub(crate) fn path(&self) -> &Path {
        &self.inner.path
    }

    #[instrument(level = "trace", skip(self))]
    pub(crate) fn source(&self) -> &str {
        &self.inner.source
    }

    #[instrument(level = "trace", skip(self))]
    pub(crate) fn source_digest(&self) -> [u8; 32] {
        self.inner.digest
    }

    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    pub(crate) fn matches_disk(&self) -> CordialResult<bool> {
        matches_snapshot(&self.inner)
    }
}

impl SourceSnapshot for SharedParsedFile {
    #[instrument(level = "trace", skip(self))]
    fn path(&self) -> &Path {
        self.path()
    }

    #[instrument(level = "trace", skip(self))]
    fn source_digest(&self) -> [u8; 32] {
        self.source_digest()
    }
}

impl SourceSnapshot for SourceEditView {
    #[instrument(level = "trace", skip(self))]
    fn path(&self) -> &Path {
        self.path()
    }

    #[instrument(level = "trace", skip(self))]
    fn source_digest(&self) -> [u8; 32] {
        self.source_digest()
    }
}

impl<T: SourceSnapshot + ?Sized> SourceSnapshot for &T {
    #[instrument(level = "trace", skip(self))]
    fn path(&self) -> &Path {
        (*self).path()
    }

    #[instrument(level = "trace", skip(self))]
    fn source_digest(&self) -> [u8; 32] {
        (*self).source_digest()
    }
}
