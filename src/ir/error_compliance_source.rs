//! Snapshot-bound internal error compliance collected by the shared loader.

use crate::etiquettes::internal_error_chain::InternalErrorComplianceFinding;
use crate::loader::path_has_fixtures;
use crate::{CordialResult, FileSnapshot};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod bindings;
mod expressions;
mod native;

/// Required compliance evidence on the shared crate graph root.
pub const ATTR_SOURCE_ERROR_COMPLIANCE: &str = "source_error_compliance";

/// Complete expression compliance evidence, including files with no findings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceErrorCompliance {
    crate_root: PathBuf,
    files: Vec<ErrorComplianceFile>,
}

/// Compliance records bound to the source snapshot that supplied them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct ErrorComplianceFile {
    snapshot: FileSnapshot,
    findings: Vec<InternalErrorComplianceFinding>,
}

impl SourceErrorCompliance {
    #[tracing::instrument(level = "debug", skip(collect), err(level = "warn"))]
    pub(crate) fn collect_with(
        root: &Path,
        mut collect: impl FnMut(&Path, &Path) -> CordialResult<ErrorComplianceFile>,
    ) -> CordialResult<Self> {
        let tree = root.join("src");
        let mut files = Vec::new();
        if tree.is_dir() {
            for entry in walkdir::WalkDir::new(&tree).sort_by_file_name() {
                let entry = entry.map_err(anyhow::Error::new)?;
                if entry.file_type().is_file()
                    && entry.path().extension().is_some_and(|ext| ext == "rs")
                    && !path_has_fixtures(entry.path(), root)
                {
                    files.push(collect(entry.path(), &tree)?);
                }
            }
        }
        files.sort_by(|a, b| a.snapshot.path().cmp(b.snapshot.path()));
        Ok(Self {
            crate_root: root.to_path_buf(),
            files,
        })
    }

    /// Read compliance using only retained graph evidence.
    #[tracing::instrument(level = "debug", skip(self))]
    pub fn findings(&self, relative_paths: bool) -> Vec<InternalErrorComplianceFinding> {
        let mut findings: Vec<_> = self
            .files
            .iter()
            .flat_map(|file| file.findings.iter().cloned())
            .collect();
        findings.sort_by(|a, b| {
            a.file()
                .cmp(b.file())
                .then(a.line().cmp(&b.line()))
                .then(a.rule_id().to_string().cmp(&b.rule_id().to_string()))
        });
        if relative_paths {
            for finding in &mut findings {
                finding.strip_file_prefix(&self.crate_root);
            }
        }
        findings
    }
}
