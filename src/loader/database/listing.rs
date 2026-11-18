//! Which Rust files a crate has, as a query over the file set of its source root.
//!
//! The collectors that scan a crate used to list its files by walking its directories. The database
//! already holds them: the file set of a source root is an input, so that a file that appears or goes
//! changes what read it. The listing is a query over that input, and nothing in it reads the file
//! system.

use std::path::{Path, PathBuf};

use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use tracing::instrument;

use super::SourceDatabase;
use super::queries::SemanticDatabase;
use crate::CordialResult;
use crate::loader::held_files::HeldFiles;

/// A crate's root, and a file of the source root it is in, to ask the file set through.
#[salsa::interned(persist)]
pub(super) struct HeldKey<'db> {
    #[returns(copy)]
    pub(super) anchor: ra_ap_vfs::FileId,
    #[returns(ref)]
    pub(super) root: PathBuf,
}

/// The Rust files of the source root of `anchor` that are under the crate's root.
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn held_rust_files<'db>(
    db: &'db dyn SemanticDatabase,
    key: HeldKey<'db>,
) -> Vec<PathBuf> {
    let root_db = db.root_database();
    let set = root_db
        .file_source_root(key.anchor(db))
        .source_root_id(root_db);
    let files = root_db.source_root(set).source_root(root_db);
    let root = key.root(db);
    files
        .iter()
        .filter_map(|file| {
            let path = PathBuf::from(files.path_for_file(&file)?.as_path()?.as_str());
            (path.starts_with(root) && path.extension().is_some_and(|ext| ext == "rs"))
                .then_some(path)
        })
        .collect()
}

impl SourceDatabase {
    /// The Rust files the database holds under the crate's root.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn held_files(&self, root: &Path) -> CordialResult<HeldFiles> {
        let anchor = ["src/lib.rs", "src/main.rs"]
            .iter()
            .find_map(|entry| self.file_id(&root.join(entry)).ok())
            .or_else(|| self.any_file_under(root));
        let Some(anchor) = anchor else {
            return Ok(HeldFiles::default());
        };
        let key = HeldKey::new(&self.db, anchor, root.to_path_buf());
        Ok(HeldFiles::new(held_rust_files(&self.db, key).clone()))
    }

    /// A file the database holds under `root`: a crate with no `lib.rs` or `main.rs` of its own.
    fn any_file_under(&self, root: &Path) -> Option<ra_ap_vfs::FileId> {
        self.vfs
            .iter()
            .find(|(_, path)| {
                path.as_path()
                    .is_some_and(|path| Path::new(path.as_str()).starts_with(root))
            })
            .map(|(file, _)| file)
    }
}
