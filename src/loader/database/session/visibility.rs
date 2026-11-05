//! Collect the target's module hierarchy from canonical database trees.
use super::SourceDatabase;
use crate::{
    CordialResult,
    ir::{SourcePolicyInventory, VisibilityFacts},
    loader::DatabaseSource,
};
use ra_ap_hir::Semantics;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use std::path::Path;

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    pub(super) fn project_visibility(&self, root: &Path) -> CordialResult<VisibilityFacts> {
        let sema = Semantics::new(&self.db);
        SourcePolicyInventory::collect_native_visibility(root, |path| {
            let file = self.file_id(path)?;
            let edition = sema.attach_first_edition(file);
            if let Some(error) = edition.parse(&self.db).errors().first() {
                return Err(anyhow::Error::new(error.clone()).into());
            }
            let source = DatabaseSource::new(
                #[cfg(any(feature = "tracing", feature = "internal_error_chain"))]
                &self.db,
                path,
                self.db.file_text(file).text(&self.db),
                sema.parse(edition),
                ra_ap_ide_db::line_index(&self.db, file),
            );
            SourcePolicyInventory::native_visibility_file(&source)
        })
    }
}
