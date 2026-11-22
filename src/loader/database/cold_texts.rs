//! The texts of the files the analysis does not change, which can leave memory.
//!
//! The text of a library file (a dependency, the sysroot) is read once into the database and, for
//! most of them, not asked for again. Under memory pressure it is dropped and read from the disk if
//! something asks. A file of the workspace stays: it can be edited, and a text made up in memory has
//! nowhere to be read from.

use ra_ap_ide_db::RootDatabase;
use ra_ap_ide_db::base_db::{LibraryRoots, SourceDatabase as RaSourceDatabase};
use ra_ap_vfs::Vfs;
use tracing::instrument;

use super::SourceDatabase;

/// The files of the library roots (dependencies and the sysroot), with where each is on the disk. A
/// file a virtual path names has no place on the disk and is not one of them.
fn library_files<'a>(
    db: &'a RootDatabase,
    vfs: &'a Vfs,
) -> impl Iterator<Item = (ra_ap_vfs::FileId, std::path::PathBuf)> + 'a {
    let libraries = LibraryRoots::try_get(db);
    vfs.iter().filter_map(move |(file, path)| {
        let roots = libraries?.roots(db);
        let path = path.as_path()?;
        let root = db.file_source_root(file).source_root_id(db);
        roots
            .contains(&root)
            .then(|| std::path::PathBuf::from(path.as_str()))
            .map(|path| (file, path))
    })
}

/// What [`attach_library_sources`] found.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct AttachedTexts {
    /// Library files whose text can be read from the disk again, and so is saved as a reference.
    pub(super) files: usize,
    /// How many bytes of text those are.
    pub(super) bytes: usize,
    /// Library files that are not on the disk as the analysis read them, whose text is saved.
    pub(super) kept: usize,
}

/// Say, of every library file that is on the disk with the length of its text, where it is, so that
/// a save keeps a reference to it and not its text. The text stays in memory.
#[instrument(level = "debug", skip_all)]
pub(super) fn attach_library_sources(db: &RootDatabase, vfs: &Vfs) -> AttachedTexts {
    let mut attached = AttachedTexts::default();
    for (file, path) in library_files(db, vfs) {
        let contents = db.file_text(file).contents(db);
        if contents.attach_source(&path) {
            attached.files += 1;
            attached.bytes += contents.len();
        } else {
            attached.kept += 1;
        }
    }
    tracing::debug!(
        target: "cordial::cache",
        files = attached.files,
        bytes = attached.bytes,
        kept = attached.kept,
        "library texts to be saved as references"
    );
    attached
}
impl SourceDatabase {
    /// How many files' texts are in memory, and how many bytes they are.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub fn resident_texts(&self) -> (usize, usize) {
        self.vfs
            .iter()
            .filter_map(|(file, _)| {
                let contents = self.db.file_text(file).contents(&self.db);
                contents.is_resident().then_some(contents.len())
            })
            .fold((0, 0), |(files, bytes), len| (files + 1, bytes + len))
    }

    /// Drop the texts of the library files from memory, to be read from the disk again if something
    /// asks. Returns how many files left memory and how many bytes they were.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub fn drop_library_texts(&self) -> (usize, usize) {
        let mut dropped = (0, 0);
        for (file, path) in library_files(&self.db, &self.vfs) {
            let freed = self.db.file_text(file).contents(&self.db).make_cold(path);
            if freed > 0 {
                dropped = (dropped.0 + 1, dropped.1 + freed);
            }
        }
        dropped
    }
}
