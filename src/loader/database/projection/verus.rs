//! Project dialect facts from the canonical database trees into the graph.
use super::SourceDatabase;
use crate::{
    CordialResult,
    ir::{SourceVerus, VerusFile},
    loader::{DatabaseSource, module_path_from_src_file},
};
use ra_ap_hir::Semantics;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use std::path::Path;

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    pub(super) fn project_verus(&self, root: &Path) -> CordialResult<SourceVerus> {
        let sema = Semantics::new(&self.db);
        SourceVerus::collect_with(root, |path, tree| {
            let file = self.file_id(path)?;
            let edition = sema.attach_first_edition(file);
            if let Some(error) = edition.parse(&self.db).errors().first() {
                return Err(anyhow::Error::new(error.clone()).into());
            }
            let source = DatabaseSource::new(
                &self.db,
                path,
                self.db.file_text(file).text(&self.db),
                sema.parse(edition),
                ra_ap_ide_db::line_index(&self.db, file),
            );
            Ok(VerusFile::new(
                crate::FileSnapshot::new(path.to_path_buf(), source.source_digest()),
                source.verus_facts(&module_path_from_src_file(tree, path).join("::")),
            ))
        })
    }
}
