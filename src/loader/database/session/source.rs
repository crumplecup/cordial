//! Syntax-only collection for captured inputs, including manifest-free sources.

use std::path::Path;

use ra_ap_hir::{Semantics, attach_db};
use ra_ap_ide_db::base_db::{EditionedFileId, SourceDatabase as RaSourceDatabase};

use crate::{CordialError, CordialResult, loader::DatabaseSource};

use super::super::SourceDatabase;

impl SourceDatabase {
    /// Issue syntax from the same database that freezes source bytes. Without
    /// a loaded workspace this supplies syntax facts only, never semantic proof.
    #[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
    #[hotpath::measure]
    pub(crate) fn collect_source<T>(
        &mut self,
        path: &Path,
        supplied: Option<&str>,
        collect: impl FnOnce(&DatabaseSource<'_>) -> CordialResult<T>,
    ) -> CordialResult<T> {
        self.captured_text(path, supplied)?
            .ok_or_else(|| CordialError::not_found(path.to_path_buf()))?;
        let file = self.file_id(path)?;
        let edition = if self.workspace.is_some() {
            Semantics::new(&self.db).attach_first_edition(file)
        } else {
            EditionedFileId::current_edition(&self.db, file)
        };
        attach_db(&self.db, || {
            let parsed = edition.parse(&self.db);
            if let Some(error) = parsed.errors().first() {
                return Err(CordialError::rust_parse(
                    path.display().to_string(),
                    error.clone(),
                ));
            }
            collect(&DatabaseSource::new(
                #[cfg(any(
                    feature = "tracing",
                    feature = "internal_error_chain",
                    feature = "verus_ir",
                    feature = "panics",
                    feature = "antipatterns"
                ))]
                &self.db,
                path,
                self.db.file_text(file).text(&self.db),
                parsed.tree(),
                ra_ap_ide_db::line_index(&self.db, file),
            ))
        })
    }
}
