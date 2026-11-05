//! Snapshot-bound chain evidence issued by the shared source loader.

use crate::etiquettes::error_chain::ErrorChainRecord;
use crate::loader::{SourceLoadView, path_has_fixtures};
use crate::{CordialResult, FileSnapshot};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Required crate-wide chain inventory on the shared graph root.
pub const ATTR_SOURCE_ERROR_CHAINS: &str = "source_error_chains";

/// Complete chain evidence, including source files with no records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceErrorChains {
    crate_root: PathBuf,
    files: Vec<ErrorChainsFile>,
}

/// Chain records bound to their original source snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct ErrorChainsFile {
    snapshot: FileSnapshot,
    records: Vec<ErrorChainRecord>,
}

impl SourceErrorChains {
    #[tracing::instrument(level = "debug", skip(source), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceErrorChains")]
    pub(crate) fn collect(source: &SourceLoadView, root: &Path) -> CordialResult<Self> {
        let tree = root.join("src");
        let mut files = Vec::new();
        for file in source.parsed_rust_files_under(&tree, "source-error-chains", |path| {
            !path_has_fixtures(path, root)
        })? {
            let records = crate::loader::source_walk(file.path(), "source-error-chains", || {
                crate::etiquettes::error_chain::scan_rust_syntax(&file, &tree, root)
            })?;
            files.push(ErrorChainsFile {
                snapshot: FileSnapshot::new(file.path().to_path_buf(), file.source_digest()),
                records,
            });
        }
        files.sort_by(|a, b| a.snapshot.path().cmp(b.snapshot.path()));
        Ok(Self {
            crate_root: root.to_path_buf(),
            files,
        })
    }

    /// Read deterministic rows without consulting syntax or the filesystem.
    #[tracing::instrument(level = "debug", skip(self))]
    pub fn records(&self) -> Vec<ErrorChainRecord> {
        let mut records: Vec<_> = self
            .files
            .iter()
            .flat_map(|file| file.records.clone())
            .collect();
        records.sort_by(|a, b| {
            a.file()
                .cmp(b.file())
                .then(a.line().cmp(&b.line()))
                .then(a.rule_id().as_str().cmp(b.rule_id().as_str()))
        });
        records
    }
}
