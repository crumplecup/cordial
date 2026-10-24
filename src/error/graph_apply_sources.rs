//! Graph errors specific to error-field apply plans.

use std::panic::Location;

use tracing::instrument;

/// An apply plan was built from source bytes that are no longer on disk.
#[derive(Debug, derive_getters::Getters)]
pub struct StaleSourceSnapshotSource {
    path: std::path::PathBuf,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl StaleSourceSnapshotSource {
    #[track_caller]
    #[instrument(level = "debug", skip(path))]
    pub fn new(path: &std::path::Path) -> Self {
        let caller = Location::caller();
        Self {
            path: path.to_path_buf(),
            file: caller.file().to_string(),
            line: caller.line(),
        }
    }
}

impl std::fmt::Display for StaleSourceSnapshotSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "source changed after collection: {} ({}:{})",
            self.path.display(),
            self.file,
            self.line
        )
    }
}

impl std::error::Error for StaleSourceSnapshotSource {}

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
