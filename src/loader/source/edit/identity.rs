//! Identity-only access for error conversion planning.

use super::super::SharedParsedFile;
use super::SourceEditView;
use crate::FileSnapshot;
use std::path::Path;
use tracing::instrument;

/// Snapshot identity shared by collection and source-only planning views.
pub(crate) trait SourceSnapshot {
    fn path(&self) -> &Path;
    fn source_digest(&self) -> [u8; 32];
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
