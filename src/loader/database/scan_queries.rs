//! The per-file half of each whole-crate collector, as Salsa queries.
//!
//! A collector walks the crate's files and merges one result per file. The
//! result for a file is a function of that file's text and parse (and of the
//! scan roots it is read under), so each is a tracked query keyed by the file:
//! a session computes it again only for a file whose text changed. The walk and
//! the merge stay where they were; they cost a copy, not an analysis.

use std::path::PathBuf;

use ra_ap_hir::Semantics;
use ra_ap_ide_db::RootDatabase;
use tracing::instrument;

use super::DatabaseSource;
use super::queries::SemanticDatabase;

/// A file as one collector reads it: the database's id for it, the path the IR
/// names it by, the scan tree it was found under, and the crate root.
#[salsa::interned]
pub(super) struct ScanKey<'db> {
    #[returns(copy)]
    file: ra_ap_vfs::FileId,
    #[returns(ref)]
    path: PathBuf,
    #[returns(ref)]
    tree: PathBuf,
    #[returns(ref)]
    root: PathBuf,
}

/// Build the file's carrier and run `collect` on it, or `None` when the file
/// does not parse or the collector refuses it. The caller falls back to the
/// uncached path in that case, which reports the error.
#[instrument(level = "debug", skip(db, collect), fields(query = query))]
#[hotpath::measure]
fn scanned<T>(
    db: &RootDatabase,
    key: (ra_ap_vfs::FileId, &std::path::Path),
    query: &str,
    collect: impl FnOnce(&DatabaseSource<'_>) -> crate::CordialResult<T>,
) -> Option<T> {
    let (file, path) = key;
    let sema = Semantics::new(db);
    let source = super::projection::source_of(db, &sema, file, path).ok()?;
    tracing::debug!(target: "cordial::semantic::queries", function = %path.display(), query, "semantic query executed");
    collect(&source).ok()
}

#[cfg(feature = "error_sites")]
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn error_sites_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: ScanKey<'db>,
) -> Option<crate::ir::ErrorSitesFile> {
    scanned(
        db.root_database(),
        (key.file(db), key.path(db)),
        "error_sites_file",
        |source| crate::ir::SourceErrorSites::native_file(source, key.tree(db), key.root(db)),
    )
}

#[cfg(feature = "error_chain")]
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn error_chains_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: ScanKey<'db>,
) -> Option<crate::ir::ErrorChainsFile> {
    scanned(
        db.root_database(),
        (key.file(db), key.path(db)),
        "error_chains_file",
        |source| crate::ir::SourceErrorChains::native_file(source, key.tree(db), key.root(db)),
    )
}

#[cfg(feature = "internal_error_chain")]
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn error_types_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: ScanKey<'db>,
) -> Option<crate::ir::ErrorTypesFile> {
    scanned(
        db.root_database(),
        (key.file(db), key.path(db)),
        "error_types_file",
        |source| crate::ir::SourceErrorTypes::native_file(source, key.tree(db)),
    )
}
