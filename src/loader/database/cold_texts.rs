//! The texts of the files the analysis does not change, which can leave memory.
//!
//! The text of a library file (a dependency, the sysroot) is read once into the database and, for
//! most of them, not asked for again. Under memory pressure it is dropped and read from the disk if
//! something asks. A file of the workspace stays: it can be edited, and a text made up in memory has
//! nowhere to be read from.

use ra_ap_ide_db::base_db::{LibraryRoots, SourceDatabase as RaSourceDatabase};
use tracing::instrument;

use super::SourceDatabase;

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
        let libraries = LibraryRoots::try_get(&self.db);
        let Some(libraries) = libraries else {
            return (0, 0);
        };
        let roots = libraries.roots(&self.db);
        let mut dropped = (0, 0);
        for (file, path) in self.vfs.iter() {
            let Some(path) = path.as_path() else { continue };
            let root = self.db.file_source_root(file).source_root_id(&self.db);
            if !roots.contains(&root) {
                continue;
            }
            let freed = self
                .db
                .file_text(file)
                .contents(&self.db)
                .make_cold(std::path::PathBuf::from(path.as_str()));
            if freed > 0 {
                dropped = (dropped.0 + 1, dropped.1 + freed);
            }
        }
        dropped
    }
}
