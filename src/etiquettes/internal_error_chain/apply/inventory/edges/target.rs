//! Snapshot-bound identities for private declaration candidates.

use crate::loader::SharedParsedFile;
use std::path::PathBuf;
use tracing::instrument;

/// The kind of top-level declaration reached by a local error edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ErrorEdgeTargetKind {
    /// A struct declaration.
    Struct,
    /// An enum declaration.
    Enum,
}

/// Identity and source snapshot of a private crate-local declaration candidate.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, derive_getters::Getters,
)]
pub struct ErrorEdgeTarget {
    declaration: PathBuf,
    #[getter(copy)]
    line: usize,
    #[getter(copy)]
    column: usize,
    #[getter(copy)]
    source_digest: [u8; 32],
    #[getter(copy)]
    kind: ErrorEdgeTargetKind,
}

impl ErrorEdgeTarget {
    #[instrument(level = "debug", skip(file, kind))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn from_shared_file(
        file: &SharedParsedFile,
        line: usize,
        column: usize,
        kind: ErrorEdgeTargetKind,
    ) -> Self {
        Self {
            declaration: file.path().to_path_buf(),
            line,
            column,
            source_digest: file.source_digest(),
            kind,
        }
    }
}
