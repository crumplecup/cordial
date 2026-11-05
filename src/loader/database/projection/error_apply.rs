//! Error apply declarations use the canonical loader source carrier.
use super::SourceDatabase;
use crate::{CordialResult, ir::SourceErrorApply};
use std::path::Path;

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    pub(super) fn project_error_apply(&self, root: &Path) -> CordialResult<SourceErrorApply> {
        SourceErrorApply::collect_with(root, &self.error_module_files(root)?, |path| {
            Ok(SourceErrorApply::native_file(&self.error_source(path)?))
        })
    }
}
