//! Source-only view and snapshot identity for error graph apply.

use std::path::Path;

use tracing::instrument;

use crate::error::CordialResult;

use super::{SharedParsedFile, SourceLoadView, matches_snapshot};
use crate::{CordialError, FileSnapshot};
use sha2::{Digest, Sha256};

/// Snapshot identity shared by collection and source-only planning views.
pub(crate) trait SourceSnapshot {
    fn path(&self) -> &Path;
    fn source_digest(&self) -> [u8; 32];
}

/// Source snapshot available to edit planning without syntax-tree access.
#[derive(Debug, Clone, derive_getters::Getters)]
pub(crate) struct SourceEditView {
    #[getter(skip)]
    snapshot: FileSnapshot,
    source: String,
}

impl SourceLoadView {
    /// Issue text without acquiring a compatibility syntax tree. The IR identity
    /// must match the frozen database input before any edit can be planned.
    #[instrument(level = "debug", skip(self, snapshot), err(level = "warn"))]
    pub(crate) fn edit_file(&self, snapshot: &FileSnapshot) -> CordialResult<SourceEditView> {
        let source = self.source_text(snapshot.path())?;
        let digest: [u8; 32] = Sha256::digest(source.as_bytes()).into();
        if digest != snapshot.digest() {
            return Err(CordialError::workspace_source_facts_missing(
                "matching error-apply source snapshot",
            ));
        }
        Ok(SourceEditView {
            snapshot: snapshot.clone(),
            source,
        })
    }
}

impl SourceEditView {
    #[instrument(level = "trace", skip(self))]
    pub(crate) fn path(&self) -> &Path {
        self.snapshot.path()
    }

    #[instrument(level = "trace", skip(self))]
    pub(crate) fn source_digest(&self) -> [u8; 32] {
        self.snapshot.digest()
    }

    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    pub(crate) fn matches_disk(&self) -> CordialResult<bool> {
        matches_snapshot(self.path(), self.source_digest())
    }
}

impl SourceSnapshot for FileSnapshot {
    #[instrument(level = "trace", skip(self))]
    fn path(&self) -> &Path {
        self.path()
    }

    #[instrument(level = "trace", skip(self))]
    fn source_digest(&self) -> [u8; 32] {
        self.digest()
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
