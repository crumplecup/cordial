//! The keys and carrier the whole-crate collectors share.
//!
//! Antipatterns and panics both read a crate as a list of files, join one fact
//! from every file's contribution, and scan each file under that fact. The
//! files, the crate, and the way a query reaches a file's syntax are the same.

use std::path::PathBuf;

use ra_ap_hir::Semantics;
use tracing::instrument;

use super::queries::SemanticDatabase;

/// A file as a whole-crate collector reads it.
#[salsa::interned(persist)]
pub(super) struct FileKey<'db> {
    #[returns(copy)]
    pub(super) file: ra_ap_vfs::FileId,
    #[returns(ref)]
    pub(super) path: PathBuf,
    /// The scan tree the file was found under.
    #[returns(ref)]
    pub(super) tree: PathBuf,
    #[returns(ref)]
    pub(super) root: PathBuf,
}

/// The files whose contributions are joined.
#[cfg(any(
    feature = "antipatterns",
    feature = "panics",
    feature = "internal_error_chain"
))]
#[salsa::interned(persist)]
pub(super) struct CrateKey<'db> {
    #[returns(ref)]
    pub(super) files: Vec<(ra_ap_vfs::FileId, PathBuf, PathBuf)>,
    #[returns(ref)]
    pub(super) root: PathBuf,
}

#[cfg(any(
    feature = "antipatterns",
    feature = "panics",
    feature = "internal_error_chain"
))]
impl<'db> CrateKey<'db> {
    /// The key of each file the crate key lists, in order.
    #[instrument(level = "trace", skip(self, db))]
    pub(super) fn file_keys(self, db: &'db dyn SemanticDatabase) -> Vec<FileKey<'db>> {
        self.files(db)
            .iter()
            .map(|(file, path, tree)| {
                FileKey::new(db, *file, path.clone(), tree.clone(), self.root(db).clone())
            })
            .collect()
    }
}

/// Run `read` on the file's syntax carrier, or `None` when the file does not
/// parse or the collector refuses it. The caller falls back to the direct path,
/// which reports why.
#[instrument(level = "trace", skip(db, key, read), fields(query = query))]
pub(super) fn carried<T>(
    db: &dyn SemanticDatabase,
    key: FileKey<'_>,
    query: &str,
    read: impl FnOnce(&crate::loader::DatabaseSource<'_>) -> crate::CordialResult<T>,
) -> Option<T> {
    let root_db = db.root_database();
    let sema = Semantics::new(root_db);
    let source = super::projection::source_of(root_db, &sema, key.file(db), key.path(db)).ok()?;
    tracing::debug!(target: "cordial::semantic::queries", function = %key.path(db).display(), query, "semantic query executed");
    read(&source).ok()
}
