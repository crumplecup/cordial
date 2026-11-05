//! Required antipattern source evidence, separated from remediation policy.

use crate::config::{ErrorFieldStrategy, StaticRefStrategy};
use crate::etiquettes::antipatterns::{
    AntipatternCatalog, AntipatternSiteRecord, AntipatternWorkspaceFacts,
};
use crate::{CordialResult, FileSnapshot};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Required crate-root attribute for policy-independent antipattern evidence.
pub const ATTR_SOURCE_ANTIPATTERNS: &str = "source_antipatterns";

/// Source facts used to evaluate antipattern policies without rescanning files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceAntipatterns {
    crate_root: PathBuf,
    #[getter(skip)]
    catalog: AntipatternCatalog,
}

impl SourceAntipatterns {
    #[tracing::instrument(level = "debug", skip(source), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceAntipatterns")]
    pub(crate) fn collect(
        source: &crate::SourceLoadView,
        root: &Path,
        crate_name: &str,
    ) -> CordialResult<Self> {
        Ok(Self {
            crate_root: root.into(),
            catalog: AntipatternCatalog::collect(source, root, crate_name)?,
        })
    }

    /// Fingerprints of every inspected source file, including empty files.
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn snapshots(&self) -> &[FileSnapshot] {
        self.catalog.snapshots()
    }

    /// Evaluate current policy and registry evidence without reading source.
    #[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
    pub fn assess(
        &self,
        crate_name: &str,
        workspace: &AntipatternWorkspaceFacts,
        static_strategy: StaticRefStrategy,
        error_strategy: ErrorFieldStrategy,
    ) -> CordialResult<Vec<AntipatternSiteRecord>> {
        self.catalog
            .assess(crate_name, workspace, static_strategy, error_strategy)
    }
}
