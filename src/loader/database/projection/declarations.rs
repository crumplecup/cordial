//! Source declaration nodes, semantic edges and their exact input identities.

use super::SourceDatabase;
use crate::loader::database::{DatabaseSource, populate::Projection};
use crate::{CordialResult, CrateIr, FileSnapshot};
use ra_ap_hir::Semantics;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
    pub(super) fn project_declarations(
        &self,
        paths: &[PathBuf],
        graph: &mut CrateIr,
    ) -> CordialResult<Vec<FileSnapshot>> {
        let mut snapshots = Vec::new();
        let mut projection = Projection::default();
        let sema = Semantics::new(&self.db);
        for path in paths {
            let file = self.file_id(path)?;
            let text = self.db.file_text(file).text(&self.db);
            snapshots.push(FileSnapshot::new(
                path.canonicalize()?,
                Sha256::digest(text.as_bytes()).into(),
            ));
            let edition = sema.attach_first_edition(file);
            // Semantics consults this same memoized parse. Recovery cannot
            // authorize a complete declaration or edit inventory.
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
                text,
                sema.parse(edition),
                ra_ap_ide_db::line_index(&self.db, file),
            );
            let module_path = sema
                .file_to_module_defs(file)
                .next()
                .map(|module| {
                    module
                        .path_to_root(&self.db)
                        .into_iter()
                        .rev()
                        .filter_map(|module| module.name(&self.db))
                        .map(|name| name.as_str().to_owned())
                        .collect::<Vec<_>>()
                        .join("::")
                })
                .unwrap_or_else(|| path.display().to_string());
            projection.populate(&self.db, &sema, &source, &module_path, graph)?;
        }
        projection.connect(graph)?;
        Ok(snapshots)
    }
}
