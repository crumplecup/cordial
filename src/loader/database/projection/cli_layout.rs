//! Collect the target's module hierarchy from canonical database trees.
use super::SourceDatabase;
use crate::{
    CordialResult,
    ir::{CliLayoutFacts, LayoutFiles, NativeFile, SourcePolicyInventory},
    loader::{DatabaseSource, ModuleTarget},
};
use ra_ap_hir::Semantics;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use std::path::{Path, PathBuf};

/// The files of a crate as the layout collection asks about them: what each contributes is a saved
/// query, and which files exist is what the database holds.
struct Held<'a> {
    database: &'a SourceDatabase,
    root: &'a Path,
    name: &'a str,
}

impl Held<'_> {
    /// Any file of the source root the crate is in, to ask its file set through.
    fn anchor(&self) -> Option<ra_ap_vfs::FileId> {
        let lib = self.root.join("src/lib.rs");
        self.database.file_id(&lib).ok()
    }
}

impl LayoutFiles for Held<'_> {
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    fn read(&mut self, path: &Path) -> CordialResult<NativeFile> {
        let db = &self.database.db;
        let file = self.database.file_id(path)?;
        let key = super::super::layout_queries::LayoutKey::new(
            db,
            super::super::crate_files::FileKey::new(
                db,
                file,
                path.to_path_buf(),
                self.root.join("src"),
                self.root.to_path_buf(),
            ),
            self.name.to_owned(),
        );
        if let Some(read) = super::super::layout_queries::cli_layout_file(db, key) {
            return Ok(read.clone());
        }
        // The query declines a file it cannot read; reading it directly says why.
        let sema = Semantics::new(db);
        let edition = super::super::edition::edition_of(db, file);
        if let Some(error) = edition.parse(db).errors().first() {
            return Err(anyhow::Error::new(error.clone()).into());
        }
        let source = DatabaseSource::new(
            #[cfg(any(
                feature = "tracing",
                feature = "internal_error_chain",
                feature = "verus_ir",
                feature = "panics",
                feature = "antipatterns"
            ))]
            db,
            path,
            db.file_text(file).text(db),
            sema.parse(edition),
            ra_ap_ide_db::line_index(db, file),
        );
        SourcePolicyInventory::native_cli_layout_file(&source, self.name)
    }

    fn follow(&mut self, from: &Path, target: &ModuleTarget) -> CordialResult<Option<PathBuf>> {
        let file = self.database.file_id(from)?;
        super::super::module_queries::follow(&self.database.db, file, target)
            .map(|found| found.map(|(_, path)| path))
            .map_err(|()| crate::CordialError::not_found(from.to_path_buf()))
    }

    fn holds(&mut self, path: &Path) -> bool {
        self.anchor().is_some_and(|anchor| {
            super::super::module_queries::holds(&self.database.db, anchor, path)
        })
    }

    fn under(&mut self, dir: &Path) -> Vec<PathBuf> {
        self.anchor()
            .map(|anchor| super::super::module_queries::held_under(&self.database.db, anchor, dir))
            .unwrap_or_default()
    }
}

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn project_cli_layout(
        &self,
        root: &Path,
        name: &str,
    ) -> CordialResult<CliLayoutFacts> {
        SourcePolicyInventory::collect_native_cli_layout(
            root,
            name,
            &mut Held {
                database: self,
                root,
                name,
            },
        )
    }
}
