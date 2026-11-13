//! The per-file half of each whole-crate collector, as Salsa queries.
//!
//! A collector walks the crate's files and merges one result per file. The
//! result for a file is a function of that file's text and parse (and of the
//! scan roots it is read under), so each is a tracked query keyed by the file:
//! a session computes it again only for a file whose text changed. The walk and
//! the merge stay where they were; they cost a copy, not an analysis.

use std::path::PathBuf;

#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir"
))]
use ra_ap_hir::Semantics;
#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir"
))]
use ra_ap_ide_db::RootDatabase;
use tracing::instrument;

#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir"
))]
use super::DatabaseSource;
#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir"
))]
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
    /// A name the collector reads the file under (the crate's), or empty.
    #[returns(ref)]
    name: String,
    /// A flag the collector reads the file under, or false.
    #[returns(copy)]
    flag: bool,
}

#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir"
))]
/// Build the file's carrier and run `collect` on it, or `None` when the file
/// does not parse or the collector refuses it. The caller falls back to the
/// uncached path in that case, which reports the error.
#[instrument(level = "debug", skip(db, collect), fields(query = query))]
#[hotpath::measure]
fn scanned<T>(
    db: &RootDatabase,
    key: (ra_ap_vfs::FileId, &std::path::Path),
    query: &str,
    collect: impl FnOnce(&DatabaseSource<'_>, &Semantics<'_, RootDatabase>) -> crate::CordialResult<T>,
) -> Option<T> {
    let (file, path) = key;
    let sema = Semantics::new(db);
    let source = super::projection::source_of(db, &sema, file, path).ok()?;
    tracing::debug!(target: "cordial::semantic::queries", function = %path.display(), query, "semantic query executed");
    collect(&source, &sema).ok()
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
        |source, _| crate::ir::SourceErrorSites::native_file(source, key.tree(db), key.root(db)),
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
        |source, _| crate::ir::SourceErrorChains::native_file(source, key.tree(db), key.root(db)),
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
        |source, _| crate::ir::SourceErrorTypes::native_file(source, key.tree(db)),
    )
}

#[cfg(feature = "internal_error_chain")]
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn error_compliance_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: ScanKey<'db>,
) -> Option<crate::ir::ErrorComplianceFile> {
    scanned(
        db.root_database(),
        (key.file(db), key.path(db)),
        "error_compliance_file",
        |source, _| {
            crate::ir::SourceErrorCompliance::native_file(
                source,
                key.tree(db),
                key.name(db),
                key.flag(db),
            )
        },
    )
}

#[cfg(feature = "tracing")]
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn observability_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: ScanKey<'db>,
) -> Option<crate::ir::ObservabilityFile> {
    let root_db = db.root_database();
    scanned(
        root_db,
        (key.file(db), key.path(db)),
        "observability_file",
        |source, sema| {
            Ok(ra_ap_hir::attach_db(root_db, || {
                crate::ir::SourceObservability::native_file(
                    source,
                    key.tree(db),
                    key.root(db),
                    &|call| super::identity::macro_by_path(sema, call),
                    &|path, scope| super::identity::macro_at(sema, scope, path),
                )
            }))
        },
    )
}

#[cfg(feature = "verus_ir")]
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn verus_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: ScanKey<'db>,
) -> Option<crate::ir::VerusFile> {
    let root_db = db.root_database();
    scanned(
        root_db,
        (key.file(db), key.path(db)),
        "verus_file",
        |source, sema| {
            Ok(ra_ap_hir::attach_db(root_db, || {
                crate::ir::VerusFile::new(
                    crate::FileSnapshot::new(key.path(db).clone(), source.source_digest()),
                    source.verus_facts(
                        &crate::loader::module_path_from_src_file(key.tree(db), key.path(db))
                            .join("::"),
                        &|call| super::identity::is_verus_macro(sema, call),
                    ),
                )
            }))
        },
    )
}
