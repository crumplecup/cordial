//! Native source for a performance record that cannot be stored or read.

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::panic::Location;
use std::path::PathBuf;

use tracing::instrument;

#[derive(Debug, derive_getters::Getters)]
pub struct PerfSource {
    path: PathBuf,
    reason: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl PerfSource {
    #[track_caller]
    #[instrument(level = "debug", skip(path, reason))]
    #[hotpath::measure]
    pub fn new(path: PathBuf, reason: impl Into<String>) -> Self {
        let location = Location::caller();
        Self {
            path,
            reason: reason.into(),
            file: location.file().to_string(),
            line: location.line(),
        }
    }
}

impl Display for PerfSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(
            formatter,
            "performance record {}: {}",
            self.path.display(),
            self.reason
        )
    }
}

impl std::error::Error for PerfSource {}

impl super::CordialError {
    /// A performance record could not be written, read or understood.
    #[track_caller]
    #[instrument(level = "debug", skip(path, reason))]
    #[hotpath::measure]
    pub fn perf(path: PathBuf, reason: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::Perf(PerfSource::new(path, reason)))
    }
}
