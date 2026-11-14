//! The module targets each source file of a package declares, as a query.
//!
//! A file's `#[path]` targets are a function of its syntax and of the package
//! it is read for, so each is a query keyed by the file and the package: an
//! edit recomputes the edited file's targets only. The workspace graph that
//! joins them is cheap and is assembled where it is asked for.

use tracing::instrument;

use super::crate_files::{FileKey, carried};
use super::queries::SemanticDatabase;
use crate::ir::PathInclusionSource;

/// A file read for one package of the workspace.
#[salsa::interned(persist)]
pub(super) struct PackageFileKey<'db> {
    #[returns(copy)]
    pub(super) file: FileKey<'db>,
    #[returns(ref)]
    pub(super) package: String,
}

/// One file's fingerprint and the module targets it declares.
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn path_inclusion_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: PackageFileKey<'db>,
) -> Option<PathInclusionSource> {
    carried(db, key.file(db), "path_inclusion_file", |source| {
        Ok(PathInclusionSource::of_file(key.package(db), source))
    })
}
