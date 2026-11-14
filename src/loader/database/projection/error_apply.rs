//! Error apply declarations use the canonical loader source carrier.
use super::SourceDatabase;
use crate::loader::database::apply_queries;
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

    /// The crate at `root` and the files apply reads of it, as a query key.
    #[tracing::instrument(level = "trace", skip(self), err(level = "warn"))]
    fn apply_key(&self, root: &Path) -> CordialResult<apply_queries::ApplyKey<'_>> {
        let files = self
            .error_module_files(root)?
            .into_iter()
            .map(|path| Ok((self.file_id(&path)?, path)))
            .collect::<CordialResult<Vec<_>>>()?;
        Ok(apply_queries::ApplyKey::new(
            &self.db,
            root.to_path_buf(),
            files,
        ))
    }

    /// The conversion unit of every candidate error field in the crate at
    /// `root`, read through the apply queries: computed the first time, and
    /// again only for the files whose facts changed.
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub fn conversion_units(
        &self,
        root: &Path,
    ) -> CordialResult<Vec<crate::etiquettes::internal_error_chain::ErrorConversionUnit>> {
        use crate::etiquettes::internal_error_chain::{type_index, units};
        let key = self.apply_key(root)?;
        let source = apply_queries::apply_source(&self.db, key)
            .clone()
            .ok_or_else(|| {
                crate::CordialError::workspace_source_facts_missing("error apply facts")
            })?;
        match apply_queries::apply_units(&self.db, key) {
            Some(found) => Ok(found.clone()),
            None => units(&source, &type_index(&source)?),
        }
    }

    /// The evidence apply plans from for the crate at `root`, from the queries:
    /// computed the first time it is asked for, and again only for what changed.
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(crate) fn apply_evidence(
        &self,
        root: &Path,
        strategy: crate::config::ErrorFieldStrategy,
    ) -> CordialResult<crate::etiquettes::internal_error_chain::ApplyEvidence> {
        use crate::etiquettes::internal_error_chain::{type_index, units};
        let key = self.apply_key(root)?;
        let missing = |what| crate::CordialError::workspace_source_facts_missing(what);
        let source = apply_queries::apply_source(&self.db, key)
            .clone()
            .ok_or_else(|| missing("error apply facts"))?;
        // A query that declines has lost why; computing the phase directly says.
        let index = match apply_queries::apply_index(&self.db, key) {
            Some(found) => found.clone(),
            None => type_index(&source)?,
        };
        let unit_list = match apply_queries::apply_units(&self.db, key) {
            Some(found) => found.clone(),
            None => units(&source, &index)?,
        };
        let plans = apply_queries::apply_graphs(
            &self.db,
            apply_queries::GraphKey::new(&self.db, key, strategy),
        )
        .clone()
        .ok_or_else(|| missing("error apply graph plans"))?;
        Ok(crate::etiquettes::internal_error_chain::ApplyEvidence::new(
            source, index, unit_list, plans,
        ))
    }
}
