//! Collect the crate's error-site census from canonical database inputs.

use super::SourceDatabase;
use crate::{CordialResult, ir::SourceErrorSites, loader::DatabaseSource};
use ra_ap_hir::Semantics;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use std::path::Path;

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    pub(super) fn project_error_sites(&self, root: &Path) -> CordialResult<SourceErrorSites> {
        let sema = Semantics::new(&self.db);
        SourceErrorSites::collect_with(root, |path, tree| {
            let file = self.file_id(path)?;
            let edition = sema.attach_first_edition(file);
            if let Some(error) = edition.parse(&self.db).errors().first() {
                return Err(anyhow::Error::new(error.clone()).into());
            }
            let source = DatabaseSource::new(
                #[cfg(feature = "tracing")]
                &self.db,
                path,
                self.db.file_text(file).text(&self.db),
                sema.parse(edition),
                ra_ap_ide_db::line_index(&self.db, file),
            );
            SourceErrorSites::native_file(&source, tree, root)
        })
    }
}
