//! Native source for refused or incomplete store cleaning.

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::panic::Location;
use std::path::PathBuf;

use tracing::instrument;

#[derive(Debug, derive_getters::Getters)]
pub struct CleanSource {
    path: PathBuf,
    reason: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl CleanSource {
    #[track_caller]
    #[instrument(level = "debug", skip(path, reason))]
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

impl Display for CleanSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(
            formatter,
            "cannot clean {}: {}",
            self.path.display(),
            self.reason
        )
    }
}

impl std::error::Error for CleanSource {}

impl super::CordialError {
    /// A requested clean was refused or incomplete.
    #[track_caller]
    #[instrument(level = "debug", skip(path, reason))]
    pub fn clean(path: PathBuf, reason: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::Clean(CleanSource::new(
            path, reason,
        )))
    }
}
