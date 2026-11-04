//! Collect crate-wide tracing evidence from canonical database trees.
use super::SourceDatabase;
use crate::{CordialResult, ir::SourceObservability, loader::DatabaseSource};
use ra_ap_hir::Semantics;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use std::path::Path;

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    pub(super) fn project_observability(&self, root: &Path) -> CordialResult<SourceObservability> {
        let sema = Semantics::new(&self.db);
        SourceObservability::collect_with(root, |path, tree| {
            let file = self.file_id(path)?;
            let edition = sema.attach_first_edition(file);
            if let Some(error) = edition.parse(&self.db).errors().first() {
                if crate::loader::path_has_fixtures(path, root) {
                    return Ok(SourceObservability::invalid_file(
                        path,
                        root,
                        self.db.file_text(file).text(&self.db),
                    ));
                }
                return Err(anyhow::Error::new(error.clone()).into());
            }
            let source = DatabaseSource::new(
                &self.db,
                path,
                self.db.file_text(file).text(&self.db),
                sema.parse(edition),
                ra_ap_ide_db::line_index(&self.db, file),
            );
            Ok(SourceObservability::native_file(&source, tree, root))
        })
    }
}
