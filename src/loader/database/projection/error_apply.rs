//! Error apply declarations use the canonical loader source carrier.
use super::SourceDatabase;
use crate::loader::database::scan_queries::{self, ScanKey};
use crate::{CordialResult, ir::SourceErrorApply};
use std::path::{Path, PathBuf};

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn project_error_apply(&self, root: &Path) -> CordialResult<SourceErrorApply> {
        SourceErrorApply::collect_with(root, &self.error_module_files(root)?, |path| {
            let key = ScanKey::new(
                &self.db,
                self.file_id(path)?,
                path.to_path_buf(),
                PathBuf::new(),
                root.to_path_buf(),
                String::new(),
                false,
            );
            match scan_queries::error_apply_file(&self.db, key) {
                Some(found) => Ok(found.clone()),
                None => Ok(SourceErrorApply::native_file(&self.error_source(path)?)),
            }
        })
    }
}
