//! Error architecture evidence as queries over the files.
//!
//! Collecting the catalog only ever adds: a file's declarations, constructors and
//! sites go in, and nothing already there is read. So each file's part is a query
//! keyed by the file, and the crate's catalog is a query that joins the parts in
//! file order. An edit recomputes the edited file's part and the join.

use tracing::instrument;

use super::crate_files::{CrateKey, FileKey, carried};
use super::queries::SemanticDatabase;
use crate::FileSnapshot;
use crate::etiquettes::internal_error_chain::ErrorArchitectureCatalog;

/// The crate and the name it is catalogued under.
#[salsa::interned]
pub(super) struct NamedCrateKey<'db> {
    #[returns(copy)]
    pub(super) files: CrateKey<'db>,
    #[returns(ref)]
    pub(super) name: String,
}

/// One file's part of the catalog, with the file's fingerprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FilePart {
    pub(super) snapshot: FileSnapshot,
    pub(super) catalog: ErrorArchitectureCatalog,
}

/// The crate's catalog and the fingerprints of the files it was joined from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Joined {
    pub(super) files: Vec<FileSnapshot>,
    pub(super) catalog: ErrorArchitectureCatalog,
}

/// What one file adds to the architecture catalog.
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn architecture_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: FileKey<'db>,
) -> Option<FilePart> {
    carried(db, key, "architecture_file", |source| {
        Ok(FilePart {
            snapshot: FileSnapshot::new(source.path().to_path_buf(), source.source_digest()),
            catalog: ErrorArchitectureCatalog::of_file(source, key.tree(db)),
        })
    })
}

/// The crate's catalog, joined from the files' parts in file order.
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn architecture_catalog<'db>(
    db: &'db dyn SemanticDatabase,
    key: NamedCrateKey<'db>,
) -> Option<Joined> {
    let files = key.files(db);
    tracing::debug!(target: "cordial::semantic::queries", function = %files.root(db).display(), query = "architecture_catalog", "semantic query executed");
    let parts = files
        .file_keys(db)
        .into_iter()
        .map(|file| architecture_file(db, file).as_ref())
        .collect::<Option<Vec<_>>>()?;
    Some(Joined {
        files: parts.iter().map(|part| part.snapshot.clone()).collect(),
        catalog: ErrorArchitectureCatalog::join(
            key.name(db),
            parts.iter().map(|part| &part.catalog),
        ),
    })
}
