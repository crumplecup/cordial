//! Collect signature-chain evidence from canonical database inputs.

use super::SourceDatabase;
use crate::loader::database::crate_files::CrateKey;
use crate::loader::database::signature_queries as queries;
use crate::{CordialResult, FileSnapshot, ir::SourceErrorSignatures, loader::DatabaseSource};
use ra_ap_hir::Semantics;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn project_error_signatures(
        &self,
        root: &std::path::Path,
    ) -> CordialResult<SourceErrorSignatures> {
        let tree = root.join("src");
        let held = self.held_files(root)?;
        let files = SourceErrorSignatures::input_paths(root, &held)
            .into_iter()
            .map(|path| Ok((self.file_id(&path)?, path, tree.clone())))
            .collect::<CordialResult<Vec<_>>>()?;
        let key = CrateKey::new(&self.db, files, root.to_path_buf());
        // A file a query declines sends the crate down the direct path, which
        // reports why.
        if let Some(joined) = queries::signature_index(&self.db, key) {
            return Ok(SourceErrorSignatures::from_parts(
                root,
                joined.files.clone(),
                joined.index.clone(),
            ));
        }
        let sema = Semantics::new(&self.db);
        SourceErrorSignatures::collect_with(root, &held, |index, path, tree| {
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
                    feature = "panics"
                ))]
                &self.db,
                path,
                self.db.file_text(file).text(&self.db),
                sema.parse(edition),
                ra_ap_ide_db::line_index(&self.db, file),
            );
            index.collect(&source, tree);
            Ok(FileSnapshot::new(
                path.to_path_buf(),
                source.source_digest(),
            ))
        })
    }
}
