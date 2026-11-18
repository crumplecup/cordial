//! What a file contributes to the CLI layout of its crate, as a saved query.
//!
//! The layout of a crate is read from every file that declares types, `act` functions or entry
//! points, and joined once the types of all of them are known. The part that needs a file's syntax is
//! a function of that syntax and the crate's name, so it is kept with the file, and the join reads
//! nothing but those parts and the set of files the database holds.

use tracing::instrument;

use super::crate_files::{FileKey, carried};
use super::queries::SemanticDatabase;
use crate::ir::{NativeFile, SourcePolicyInventory};

/// A file's contribution, for the crate it is read as.
#[salsa::interned(persist)]
pub(super) struct LayoutKey<'db> {
    #[returns(copy)]
    pub(super) file: FileKey<'db>,
    #[returns(ref)]
    pub(super) crate_name: String,
}

/// What one file contributes to the crate's CLI layout. `None` where the file cannot be read, so
/// that the caller reads it the way that says why.
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn cli_layout_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: LayoutKey<'db>,
) -> Option<NativeFile> {
    let name = key.crate_name(db);
    carried(db, key.file(db), "cli_layout_file", |source| {
        SourcePolicyInventory::native_cli_layout_file(source, name)
    })
}
