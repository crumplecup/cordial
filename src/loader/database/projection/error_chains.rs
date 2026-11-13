//! Collect the crate's error-chain evidence from canonical database inputs.

use super::SourceDatabase;
use crate::loader::database::scan_queries::{self, ScanKey};
use crate::{CordialResult, ir::SourceErrorChains};
use ra_ap_hir::Semantics;
use std::path::Path;

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn project_error_chains(&self, root: &Path) -> CordialResult<SourceErrorChains> {
        SourceErrorChains::collect_with(root, |path, tree| {
            let key = ScanKey::new(
                &self.db,
                self.file_id(path)?,
                path.to_path_buf(),
                tree.to_path_buf(),
                root.to_path_buf(),
                String::new(),
                false,
            );
            match scan_queries::error_chains_file(&self.db, key) {
                Some(found) => Ok(found.clone()),
                // The query declines a file that does not parse or that the
                // collector refuses; the direct path reports why.
                None => {
                    let sema = Semantics::new(&self.db);
                    let (_, source) = self.declaration_source(&sema, path)?;
                    SourceErrorChains::native_file(&source, tree, root)
                }
            }
        })
    }
}
