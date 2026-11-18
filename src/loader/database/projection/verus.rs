//! Project dialect facts from the canonical database trees into the graph.
use super::SourceDatabase;
use crate::loader::database::scan_queries::{self, ScanKey};
use crate::{
    CordialResult,
    ir::{SourceVerus, VerusFile},
    loader::module_path_from_src_file,
};
use ra_ap_hir::Semantics;
use std::path::Path;

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn project_verus(&self, root: &Path) -> CordialResult<SourceVerus> {
        let held = self.held_files(root)?;
        SourceVerus::collect_with(root, &held, |path, tree| {
            let key = ScanKey::new(
                &self.db,
                self.file_id(path)?,
                path.to_path_buf(),
                tree.to_path_buf(),
                root.to_path_buf(),
                String::new(),
                false,
            );
            match scan_queries::verus_file(&self.db, key) {
                Some(found) => Ok(found.clone()),
                None => {
                    let sema = Semantics::new(&self.db);
                    let (_, source) = self.declaration_source(&sema, path)?;
                    Ok(ra_ap_hir::attach_db(&self.db, || {
                        VerusFile::new(
                            crate::FileSnapshot::new(path.to_path_buf(), source.source_digest()),
                            source.verus_facts(
                                &module_path_from_src_file(tree, path).join("::"),
                                &|call| super::super::identity::is_verus_macro(&sema, call),
                            ),
                        )
                    }))
                }
            }
        })
    }
}
