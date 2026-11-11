//! Snapshot-bound conversion signatures retained for graph-only assessment.

use crate::etiquettes::internal_error_chain::{
    InternalErrorComplianceFinding, SignatureChainCheck, SignatureChainIndex,
};
use crate::loader::path_has_fixtures;
use crate::{CordialResult, FileSnapshot};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Required signature-chain evidence on the shared crate graph root.
pub const ATTR_SOURCE_ERROR_SIGNATURES: &str = "source_error_signatures";

/// Declarations and conversions from every source input, including empty files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceErrorSignatures {
    crate_root: PathBuf,
    files: Vec<FileSnapshot>,
    #[getter(skip)]
    index: SignatureChainIndex,
}

/// Complete assessments derived solely from retained signature evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct ErrorSignatureAssessment {
    findings: Vec<InternalErrorComplianceFinding>,
    checks: Vec<SignatureChainCheck>,
}

impl SourceErrorSignatures {
    #[tracing::instrument(level = "debug", skip(collect), err(level = "warn"))]
    pub(crate) fn collect_with(
        root: &Path,
        mut collect: impl FnMut(&mut SignatureChainIndex, &Path, &Path) -> CordialResult<FileSnapshot>,
    ) -> CordialResult<Self> {
        let tree = root.join("src");
        let mut files = Vec::new();
        let mut index = SignatureChainIndex::default();
        if tree.is_dir() {
            for entry in walkdir::WalkDir::new(&tree).sort_by_file_name() {
                let entry = entry.map_err(anyhow::Error::new)?;
                if entry.file_type().is_file()
                    && entry.path().extension().is_some_and(|ext| ext == "rs")
                    && !path_has_fixtures(entry.path(), root)
                {
                    files.push(collect(&mut index, entry.path(), &tree)?);
                }
            }
        }
        Ok(Self {
            crate_root: root.to_path_buf(),
            files,
            index,
        })
    }

    /// Resolve conversions from graph facts; unknown relationships stay unknown.
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceErrorSignatures")]
    pub fn assess(
        &self,
        crate_name: &str,
        relative_paths: bool,
    ) -> CordialResult<ErrorSignatureAssessment> {
        let (mut findings, mut checks) = self.index.assess(crate_name)?;
        if relative_paths {
            for finding in &mut findings {
                finding.strip_file_prefix(&self.crate_root);
            }
            for check in &mut checks {
                check.strip_file_prefix(&self.crate_root);
            }
        }
        Ok(ErrorSignatureAssessment { findings, checks })
    }
}
