//! Collect signature-chain evidence from canonical database inputs.

use super::SourceDatabase;
use crate::{CordialResult, FileSnapshot, ir::SourceErrorSignatures, loader::DatabaseSource};
use ra_ap_hir::Semantics;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    pub(super) fn project_error_signatures(
        &self,
        root: &std::path::Path,
    ) -> CordialResult<SourceErrorSignatures> {
        let sema = Semantics::new(&self.db);
        SourceErrorSignatures::collect_with(root, |index, path, tree| {
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
            index.collect_native(&source, tree);
            Ok(FileSnapshot::new(
                path.to_path_buf(),
                source.source_digest(),
            ))
        })
    }
}
