//! Loader-owned snapshot evidence for error strategy transformations.

use crate::etiquettes::internal_error_chain::{
    ConversionFileFacts, GetterConsumerFacts, GetterContractFacts, GraphDeclarationFacts,
    GraphExprFacts, GraphLifetimeFacts, InitializerFileFacts, LifetimeFileFacts, NameAudit,
};
use crate::loader::{DatabaseSource, SourceSnapshot};
use crate::{CordialError, CordialResult, FileSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

mod namespace;
mod static_strings;
mod uses;
use namespace::ApplyNamespace;
use uses::ApplyUses;

/// Required transformation evidence on the shared crate graph root.
pub const ATTR_SOURCE_ERROR_APPLY: &str = "source_error_apply";

/// Error graph declarations and safety facts for every inspected snapshot.
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
    #[getter(skip)]
    uses: ApplyUses,
    #[getter(skip)]
    expressions: GraphExprFacts,
    #[getter(skip)]
    lifetimes: GraphLifetimeFacts,
    #[getter(skip)]
    getters: GetterContractFacts,
    #[getter(skip)]
    getter_uses: GetterConsumerFacts,
    #[getter(skip)]
    local_lifetimes: LifetimeFileFacts,
    #[getter(skip)]
    initializers: InitializerFileFacts,
    #[getter(skip)]
    conversion_sites: ConversionFileFacts,
}

impl SourceErrorApply {
    #[tracing::instrument(level = "trace", skip(self, file), err(level = "warn"))]
    pub(crate) fn conversion_sites_for(
        &self,
        file: &impl SourceSnapshot,
    ) -> CordialResult<&ConversionFileFacts> {
        Ok(&self.file_for(file)?.conversion_sites)
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
            uses: ApplyUses::collect_native(file),
            expressions: GraphExprFacts::collect_native(file),
            lifetimes: GraphLifetimeFacts::collect_native(file),
            getters: GetterContractFacts::collect_native(file),
            getter_uses: GetterConsumerFacts::collect_native(file),
            local_lifetimes: LifetimeFileFacts::collect_native(file),
            initializers: InitializerFileFacts::collect_native(file),
            conversion_sites: ConversionFileFacts::collect_native(file),
        }
    }

    #[tracing::instrument(level = "trace", skip(self, file), err(level = "warn"))]
    pub(crate) fn declarations_for(
        &self,
        file: &impl SourceSnapshot,
    ) -> CordialResult<GraphDeclarationFacts> {
        Ok(self.file_for(file)?.declarations.clone())
    }

    #[tracing::instrument(level = "trace", skip(self, file), err(level = "warn"))]
    pub(crate) fn namespace_for(
        &self,
        file: &impl SourceSnapshot,
    ) -> CordialResult<&ApplyNamespace> {
        Ok(&self.file_for(file)?.namespace)
    }

    #[tracing::instrument(level = "trace", skip(self, file), err(level = "warn"))]
    pub(crate) fn name_audit_for(&self, file: &impl SourceSnapshot) -> CordialResult<&NameAudit> {
        Ok(&self.file_for(file)?.name_audit)
    }

    #[tracing::instrument(level = "trace", skip(self, file), err(level = "warn"))]
    pub(crate) fn static_strings_for(
        &self,
        file: &impl SourceSnapshot,
    ) -> CordialResult<&BTreeSet<String>> {
        Ok(&self.file_for(file)?.static_strings)
    }

    #[tracing::instrument(level = "trace", skip(self, file), err(level = "warn"))]
    fn file_for(&self, file: &impl SourceSnapshot) -> CordialResult<&ErrorApplyFile> {
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

    #[tracing::instrument(level = "trace", skip(self, file), err(level = "warn"))]
    pub(crate) fn uses_for(&self, file: &impl SourceSnapshot) -> CordialResult<&ApplyUses> {
        Ok(&self.file_for(file)?.uses)
    }
    #[tracing::instrument(level = "trace", skip(self, file), err(level = "warn"))]
    pub(crate) fn expressions_for(
        &self,
        file: &impl SourceSnapshot,
    ) -> CordialResult<&GraphExprFacts> {
        Ok(&self.file_for(file)?.expressions)
    }
    #[tracing::instrument(level = "trace", skip(self, file), err(level = "warn"))]
    pub(crate) fn lifetimes_for(
        &self,
        file: &impl SourceSnapshot,
    ) -> CordialResult<&GraphLifetimeFacts> {
        Ok(&self.file_for(file)?.lifetimes)
    }

    #[tracing::instrument(level = "trace", skip(self, file), err(level = "warn"))]
    pub(crate) fn getters_for(
        &self,
        file: &impl SourceSnapshot,
    ) -> CordialResult<&GetterContractFacts> {
        Ok(&self.file_for(file)?.getters)
    }

    #[tracing::instrument(level = "trace", skip(self, file), err(level = "warn"))]
    pub(crate) fn local_lifetimes_for(
        &self,
        file: &impl SourceSnapshot,
    ) -> CordialResult<&LifetimeFileFacts> {
        Ok(&self.file_for(file)?.local_lifetimes)
    }

    #[tracing::instrument(level = "trace", skip(self, file), err(level = "warn"))]
    pub(crate) fn initializers_for(
        &self,
        file: &impl SourceSnapshot,
    ) -> CordialResult<&InitializerFileFacts> {
        Ok(&self.file_for(file)?.initializers)
    }

    #[tracing::instrument(level = "trace", skip(self, file), err(level = "warn"))]
    pub(crate) fn getter_uses_for(
        &self,
        file: &impl SourceSnapshot,
    ) -> CordialResult<&GetterConsumerFacts> {
        Ok(&self.file_for(file)?.getter_uses)
    }
}
