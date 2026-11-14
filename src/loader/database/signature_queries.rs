//! Error signature evidence as queries over the files.
//!
//! Like the architecture catalog, the signature index is only ever added to: each
//! file's part is a query keyed by the file, and the crate's index is a query that
//! joins the parts in file order.

use tracing::instrument;

use super::crate_files::{CrateKey, FileKey, carried};
use super::queries::SemanticDatabase;
use crate::FileSnapshot;
use crate::etiquettes::internal_error_chain::SignatureChainIndex;

/// One file's part of the index, with the file's fingerprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FilePart {
    pub(super) snapshot: FileSnapshot,
    pub(super) index: SignatureChainIndex,
}

/// The crate's index and the fingerprints of the files it was joined from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Joined {
    pub(super) files: Vec<FileSnapshot>,
    pub(super) index: SignatureChainIndex,
}

/// What one file adds to the signature index.
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn signature_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: FileKey<'db>,
) -> Option<FilePart> {
    carried(db, key, "signature_file", |source| {
        Ok(FilePart {
            snapshot: FileSnapshot::new(source.path().to_path_buf(), source.source_digest()),
            index: SignatureChainIndex::of_file(source, key.tree(db)),
        })
    })
}

/// The crate's index, joined from the files' parts in file order.
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn signature_index<'db>(
    db: &'db dyn SemanticDatabase,
    key: CrateKey<'db>,
) -> Option<Joined> {
    tracing::debug!(target: "cordial::semantic::queries", function = %key.root(db).display(), query = "signature_index", "semantic query executed");
    let parts = key
        .file_keys(db)
        .into_iter()
        .map(|file| signature_file(db, file).as_ref())
        .collect::<Option<Vec<_>>>()?;
    Some(Joined {
        files: parts.iter().map(|part| part.snapshot.clone()).collect(),
        index: SignatureChainIndex::join(parts.iter().map(|part| &part.index)),
    })
}
