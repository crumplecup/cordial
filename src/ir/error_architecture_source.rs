//! Required, parser-independent error architecture evidence on the shared graph.

use crate::config::ErrorFieldStrategy;
use crate::etiquettes::internal_error_chain::{
    ErrorArchitectureCatalog, InternalErrorComplianceFinding,
};
use crate::loader::SourceLoadView;
use crate::{CordialResult, FileSnapshot};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Complete architecture inventory, including inputs without declarations.
pub const ATTR_SOURCE_ERROR_ARCHITECTURE: &str = "source_error_architecture";

/// Field contracts, constructors, type membership and fan-out relationships.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceErrorArchitecture {
    crate_root: PathBuf,
    files: Vec<FileSnapshot>,
    #[getter(skip)]
    catalog: ErrorArchitectureCatalog,
}

impl SourceErrorArchitecture {
    #[tracing::instrument(level = "debug", skip(source), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceErrorArchitecture")]
    pub(crate) fn collect(
        source: &SourceLoadView,
        root: &Path,
        crate_name: &str,
    ) -> CordialResult<Self> {
        let mut paths = Vec::new();
        crate::etiquettes::internal_error_chain::for_each_shared_src_rust_file(
            source,
            root,
            |path, _| {
                paths.push(path.to_path_buf());
                Ok(())
            },
        )?;
        Self::collect_with(root, crate_name, &paths, |catalog, path, tree| {
            let file = source.parsed_file(path, "source-error-architecture")?;
            catalog.collect(&file, tree)?;
            Ok(FileSnapshot::new(path.to_path_buf(), file.source_digest()))
        })
    }

    #[tracing::instrument(level = "debug", skip(collect), err(level = "warn"))]
    pub(crate) fn collect_with(
        root: &Path,
        crate_name: &str,
        paths: &[PathBuf],
        mut collect: impl FnMut(
            &mut ErrorArchitectureCatalog,
            &Path,
            &Path,
        ) -> CordialResult<FileSnapshot>,
    ) -> CordialResult<Self> {
        let tree = root.join("src");
        let mut files = Vec::new();
        let mut catalog = ErrorArchitectureCatalog::new(crate_name);
        for path in paths {
            files.push(collect(&mut catalog, path, &tree)?);
        }
        Ok(Self {
            crate_root: root.to_path_buf(),
            files,
            catalog,
        })
    }

    /// Evaluate a policy using graph evidence alone; no source access is needed.
    #[tracing::instrument(level = "debug", skip(self, strategy), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceErrorArchitecture")]
    pub fn assess(
        &self,
        max_call_sites: usize,
        strategy: ErrorFieldStrategy,
        relative_paths: bool,
    ) -> CordialResult<Vec<InternalErrorComplianceFinding>> {
        let mut findings = self.catalog.findings(max_call_sites, strategy)?;
        if relative_paths {
            for finding in &mut findings {
                finding.strip_file_prefix(&self.crate_root);
            }
        }
        Ok(findings)
    }
}
