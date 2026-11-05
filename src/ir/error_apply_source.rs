//! Loader-owned declaration evidence for error strategy transformations.

use crate::etiquettes::internal_error_chain::{GraphDeclarationFacts, NameAudit};
use crate::loader::{DatabaseSource, SharedParsedFile, SourceLoadView};
use crate::{CordialError, CordialResult, FileSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

mod namespace;
mod static_strings;
use namespace::ApplyNamespace;

/// Required declaration evidence on the shared crate graph root.
pub const ATTR_SOURCE_ERROR_APPLY: &str = "source_error_apply";

/// Error graph declarations, bound to every inspected source snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceErrorApply {
    crate_root: PathBuf,
    files: Vec<ErrorApplyFile>,
}

/// Original field, variant, lifetime, edge and edit-range facts for one file.
#[derive(Debug, Clone, Serialize, Deserialize, derive_getters::Getters)]
pub struct ErrorApplyFile {
    snapshot: FileSnapshot,
    #[getter(skip)]
    declarations: GraphDeclarationFacts,
    #[getter(skip)]
    namespace: ApplyNamespace,
    #[getter(skip)]
    name_audit: NameAudit,
    #[getter(skip)]
    static_strings: BTreeSet<String>,
}

impl SourceErrorApply {
    #[tracing::instrument(level = "debug", skip(source), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceErrorApply")]
    pub(crate) fn collect(source: &SourceLoadView, root: &Path) -> CordialResult<Self> {
        let mut files = Vec::new();
        crate::etiquettes::internal_error_chain::for_each_shared_src_rust_file(
            source,
            root,
            |path, _| {
                let file = source.parsed_file(path, "source-error-apply")?;
                files.push(ErrorApplyFile {
                    snapshot: FileSnapshot::new(path.to_path_buf(), file.source_digest()),
                    declarations: GraphDeclarationFacts::collect(&file),
                    namespace: ApplyNamespace::collect(&file),
                    name_audit: NameAudit::collect(&file),
                    static_strings: static_strings::collect(&file),
                });
                Ok(())
            },
        )?;
        files.sort_by(|a, b| a.snapshot.path().cmp(b.snapshot.path()));
        Ok(Self {
            crate_root: root.to_path_buf(),
            files,
        })
    }

    #[tracing::instrument(level = "debug", skip(collect), err(level = "warn"))]
    pub(crate) fn collect_with(
        root: &Path,
        paths: &[PathBuf],
        mut collect: impl FnMut(&Path) -> CordialResult<ErrorApplyFile>,
    ) -> CordialResult<Self> {
        let mut files = paths
            .iter()
            .map(|path| collect(path))
            .collect::<CordialResult<Vec<_>>>()?;
        files.sort_by(|a, b| a.snapshot.path().cmp(b.snapshot.path()));
        Ok(Self {
            crate_root: root.to_path_buf(),
            files,
        })
    }

    #[tracing::instrument(level = "debug", skip(file))]
    pub(crate) fn native_file(file: &DatabaseSource<'_>) -> ErrorApplyFile {
        ErrorApplyFile {
            snapshot: FileSnapshot::new(file.path().to_path_buf(), file.source_digest()),
            declarations: GraphDeclarationFacts::collect_native(file),
            namespace: ApplyNamespace::collect_native(file),
            name_audit: NameAudit::collect_native(file),
            static_strings: static_strings::collect_native(file),
        }
    }

    #[tracing::instrument(level = "trace", skip(self, file), err(level = "warn"))]
    pub(crate) fn declarations_for(
        &self,
        file: &SharedParsedFile,
    ) -> CordialResult<GraphDeclarationFacts> {
        Ok(self.file_for(file)?.declarations.clone())
    }

    #[tracing::instrument(level = "trace", skip(self, file), err(level = "warn"))]
    pub(crate) fn namespace_for(&self, file: &SharedParsedFile) -> CordialResult<&ApplyNamespace> {
        Ok(&self.file_for(file)?.namespace)
    }

    #[tracing::instrument(level = "trace", skip(self, file), err(level = "warn"))]
    pub(crate) fn name_audit_for(&self, file: &SharedParsedFile) -> CordialResult<&NameAudit> {
        Ok(&self.file_for(file)?.name_audit)
    }

    #[tracing::instrument(level = "trace", skip(self, file), err(level = "warn"))]
    pub(crate) fn static_strings_for(
        &self,
        file: &SharedParsedFile,
    ) -> CordialResult<&BTreeSet<String>> {
        Ok(&self.file_for(file)?.static_strings)
    }

    #[tracing::instrument(level = "trace", skip(self, file), err(level = "warn"))]
    fn file_for(&self, file: &SharedParsedFile) -> CordialResult<&ErrorApplyFile> {
        let mut matches = self
            .files
            .iter()
            .filter(|entry| entry.snapshot.path() == file.path());
        let entry = matches
            .next()
            .filter(|entry| entry.snapshot.digest() == file.source_digest())
            .ok_or_else(|| {
                CordialError::workspace_source_facts_missing("matching error-apply source snapshot")
            })?;
        if matches.next().is_some() {
            return Err(CordialError::workspace_source_facts_missing(
                "unambiguous error-apply source snapshot",
            ));
        }
        Ok(entry)
    }
}
