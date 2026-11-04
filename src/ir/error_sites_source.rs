//! Snapshot-bound error-site census collected by the shared source loader.

use crate::etiquettes::error_sites::ErrorSiteRecord;
use crate::loader::{DatabaseSource, SourceLoadView, path_has_fixtures, quality_scan_trees};
use crate::{CordialResult, FileSnapshot};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod native;

/// Required crate-wide error-site inventory on the shared graph root.
pub const ATTR_SOURCE_ERROR_SITES: &str = "source_error_sites";

/// Complete error-site evidence, including files with no sites.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceErrorSites {
    crate_root: PathBuf,
    files: Vec<ErrorSitesFile>,
}

/// Error-site records married to the source snapshot that produced them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct ErrorSitesFile {
    snapshot: FileSnapshot,
    sites: Vec<ErrorSiteRecord>,
}

impl SourceErrorSites {
    #[tracing::instrument(level = "debug", skip(source), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceErrorSites")]
    pub(crate) fn collect(source: &SourceLoadView, root: &Path) -> CordialResult<Self> {
        Self::collect_with(root, |path, tree| {
            let file = source.parsed_file(path, "source-error-sites")?;
            Ok(ErrorSitesFile {
                snapshot: FileSnapshot::new(path.to_path_buf(), file.source_digest()),
                sites: crate::loader::source_walk(path, "source-error-sites", || {
                    crate::etiquettes::error_sites::scan_rust_syntax(&file, tree, root)
                })?,
            })
        })
    }

    #[tracing::instrument(level = "debug", skip(collect), err(level = "warn"))]
    pub(crate) fn collect_with(
        root: &Path,
        mut collect: impl FnMut(&Path, &Path) -> CordialResult<ErrorSitesFile>,
    ) -> CordialResult<Self> {
        let mut files = Vec::new();
        for tree in quality_scan_trees(root) {
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

    /// Project sorted rows without consulting source files or a syntax database.
    #[tracing::instrument(level = "debug", skip(self))]
    pub fn records(&self) -> Vec<ErrorSiteRecord> {
        let mut rows: Vec<_> = self
            .files
            .iter()
            .flat_map(|file| file.sites.clone())
            .collect();
        rows.sort_by(|a, b| {
            a.file()
                .cmp(b.file())
                .then(a.line().cmp(&b.line()))
                .then(a.kind().to_string().cmp(&b.kind().to_string()))
                .then(a.source_snippet().cmp(b.source_snippet()))
        });
        rows
    }

    #[tracing::instrument(level = "debug", skip(file), err(level = "warn"))]
    pub(crate) fn native_file(
        file: &DatabaseSource<'_>,
        tree: &Path,
        root: &Path,
    ) -> CordialResult<ErrorSitesFile> {
        Ok(ErrorSitesFile {
            snapshot: FileSnapshot::new(file.path().to_path_buf(), file.source_digest()),
            sites: native::collect(file, tree, root)?,
        })
    }
}
