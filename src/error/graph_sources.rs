//! Leaf native sources for [`super::GraphErrorKind`] -- split out of
//! `graph.rs` once the combined leaf-source + Kind/Error/wrapper file
//! grew past a human reader's single-take size.

use std::panic::Location;

use tracing::instrument;

/// A field conversion requested facts that were absent from the shared IR.
#[derive(Debug, derive_getters::Getters)]
pub struct MissingConversionUnitSource {
    path: std::path::PathBuf,
    type_name: String,
    field_name: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl MissingConversionUnitSource {
    #[track_caller]
    #[instrument(level = "debug", skip(path, type_name, field_name))]
    pub fn new(path: &std::path::Path, type_name: &str, field_name: &str) -> Self {
        let caller = Location::caller();
        Self {
            path: path.to_path_buf(),
            type_name: type_name.to_string(),
            field_name: field_name.to_string(),
            file: caller.file().to_string(),
            line: caller.line(),
        }
    }
}

impl std::fmt::Display for MissingConversionUnitSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "shared IR has no conversion unit for {}::{} in {} ({}:{})",
            self.type_name,
            self.field_name,
            self.path.display(),
            self.file,
            self.line
        )
    }
}

impl std::error::Error for MissingConversionUnitSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct EdgeReferencesMissingNodeSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl EdgeReferencesMissingNodeSource {
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn new() -> Self {
        let loc = Location::caller();
        Self {
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for EdgeReferencesMissingNodeSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "edge references missing node ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for EdgeReferencesMissingNodeSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct SetAttrOnMissingNodeSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl SetAttrOnMissingNodeSource {
    #[track_caller]
    #[instrument(level = "trace")]
    pub fn new() -> Self {
        let loc = Location::caller();
        Self {
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for SetAttrOnMissingNodeSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "set_attr on missing node ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for SetAttrOnMissingNodeSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct SnapshotEdgeMissingEndpointsSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl SnapshotEdgeMissingEndpointsSource {
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn new() -> Self {
        let loc = Location::caller();
        Self {
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for SnapshotEdgeMissingEndpointsSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "snapshot edge missing endpoints ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for SnapshotEdgeMissingEndpointsSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct SnapshotEdgeFromInvalidSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl SnapshotEdgeFromInvalidSource {
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn new() -> Self {
        let loc = Location::caller();
        Self {
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for SnapshotEdgeFromInvalidSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "snapshot edge from invalid ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for SnapshotEdgeFromInvalidSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct SnapshotEdgeToInvalidSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl SnapshotEdgeToInvalidSource {
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn new() -> Self {
        let loc = Location::caller();
        Self {
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for SnapshotEdgeToInvalidSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "snapshot edge to invalid ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for SnapshotEdgeToInvalidSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct CrateMustExistSource {
    #[getter(skip)]
    crate_name: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl CrateMustExistSource {
    #[track_caller]
    #[instrument(level = "debug", skip(crate_name))]
    pub fn new(crate_name: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            crate_name: crate_name.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for CrateMustExistSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "crate `{}` must exist ({}:{})",
            self.crate_name, self.file, self.line
        )
    }
}

impl std::error::Error for CrateMustExistSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct NoLoadViewForEnricherSource {
    #[getter(skip)]
    enricher_id: String,
    #[getter(skip)]
    crate_name: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl NoLoadViewForEnricherSource {
    #[track_caller]
    #[instrument(level = "debug", skip(enricher_id, crate_name))]
    pub fn new(enricher_id: impl Into<String>, crate_name: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            enricher_id: enricher_id.into(),
            crate_name: crate_name.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for NoLoadViewForEnricherSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "no load view for enricher `{}` on crate `{}` ({}:{})",
            self.enricher_id, self.crate_name, self.file, self.line
        )
    }
}

impl std::error::Error for NoLoadViewForEnricherSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct NoLoadViewAvailableForEnricherSource {
    #[getter(skip)]
    enricher_id: String,
    #[getter(skip)]
    crate_name: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl NoLoadViewAvailableForEnricherSource {
    #[track_caller]
    #[instrument(level = "debug", skip(enricher_id, crate_name))]
    pub fn new(enricher_id: impl Into<String>, crate_name: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            enricher_id: enricher_id.into(),
            crate_name: crate_name.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for NoLoadViewAvailableForEnricherSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "no load view available for enricher `{}` on crate `{}` ({}:{})",
            self.enricher_id, self.crate_name, self.file, self.line
        )
    }
}

impl std::error::Error for NoLoadViewAvailableForEnricherSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct SourceEnricherWrongLoaderSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl SourceEnricherWrongLoaderSource {
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn new() -> Self {
        let loc = Location::caller();
        Self {
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for SourceEnricherWrongLoaderSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "source enricher received another loader ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for SourceEnricherWrongLoaderSource {}
