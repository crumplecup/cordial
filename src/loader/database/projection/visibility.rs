//! Collect the target's module hierarchy from canonical database trees.
use super::SourceDatabase;
use crate::loader::database::scan_queries::{self, ScanKey};
use crate::{
    CordialResult,
    ir::{SourcePolicyInventory, VisibilityFacts},
    loader::DatabaseSource,
};
use ra_ap_hir::Semantics;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use std::path::{Path, PathBuf};

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn project_visibility(&self, root: &Path) -> CordialResult<VisibilityFacts> {
        SourcePolicyInventory::collect_native_visibility(root, |path| {
            let key = ScanKey::new(
                &self.db,
                self.file_id(path)?,
                path.to_path_buf(),
                PathBuf::new(),
                root.to_path_buf(),
                String::new(),
                false,
            );
            match scan_queries::visibility_file(&self.db, key) {
                Some(found) => Ok(found.clone()),
                None => self.visibility_direct(path),
            }
        })
    }
}

impl SourceDatabase {
    /// One file's visibility body computed outside the query, for a file the
    /// query declined, which also reports why.
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn visibility_direct(
        &self,
        path: &Path,
    ) -> CordialResult<(crate::ir::VisibilityBody, crate::FileSnapshot)> {
        let sema = Semantics::new(&self.db);
        let file = self.file_id(path)?;
        let edition = super::super::edition::edition_of(&self.db, file);
        if let Some(error) = edition.parse(&self.db).errors().first() {
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
            &self.db,
            path,
            self.db.file_text(file).text(&self.db),
            sema.parse(edition),
            ra_ap_ide_db::line_index(&self.db, file),
        );
        #[cfg(feature = "verus_ir")]
        return ra_ap_hir::attach_db(&self.db, || {
            SourcePolicyInventory::native_visibility_file(&source, &|call| {
                super::super::identity::is_verus_macro(&sema, call)
            })
        });
        #[cfg(not(feature = "verus_ir"))]
        SourcePolicyInventory::native_visibility_file(&source)
    }
}
