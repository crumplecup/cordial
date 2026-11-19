//! The edition a file is parsed as, read without building any definition map.
//!
//! `Semantics::attach_first_edition` finds the file's module to learn its crate, which builds the
//! crate's definition map and every definition map under it, the sysroot's included. A file's
//! edition is its crate's, and the crate is one of the crates its source root holds: an input.
//! A run that only parses files (to scan them, or to read their saved facts) needs none of that.

use ra_ap_hir::EditionedFileId;
use ra_ap_ide_db::RootDatabase;
use ra_ap_ide_db::base_db::relevant_crates;
use ra_ap_vfs::FileId;
use tracing::instrument;

/// `file` with the edition of the first crate that has it, or the current edition for a file no
/// crate has.
#[instrument(level = "trace", skip(db))]
#[hotpath::measure]
pub(super) fn edition_of(db: &RootDatabase, file: FileId) -> EditionedFileId {
    let crates = relevant_crates(db, file);
    if crates.len() > 1 {
        // A file of several crates (a cfg context names the files it does not change as the
        // original's): which crate is first decides the edition, so which it is is said.
        tracing::debug!(
            target: "cordial::semantic::crates",
            file = file.index(),
            crates = crates.len(),
            first = ?crates.first(),
            "a file is in several crates: the first gives its edition"
        );
    }
    match crates.first() {
        Some(krate) => EditionedFileId::new(db, file, krate.data(db).edition),
        None => EditionedFileId::current_edition(db, file),
    }
}
