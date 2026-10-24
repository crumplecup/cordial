//! Native source for an invalid system clock conversion.

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::panic::Location;
use std::time::SystemTimeError;

use tracing::instrument;

#[derive(Debug, derive_getters::Getters)]
pub struct ClockSource {
    #[getter(skip)]
    source: SystemTimeError,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl ClockSource {
    #[track_caller]
    #[instrument(level = "debug", skip(source))]
    pub fn new(source: SystemTimeError) -> Self {
        let loc = Location::caller();
        Self {
            source,
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl From<SystemTimeError> for ClockSource {
    #[track_caller]
    #[instrument(level = "debug", skip(source))]
    fn from(source: SystemTimeError) -> Self {
        Self::new(source)
    }
}

impl Display for ClockSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "system clock error: {}", self.source)
    }
}

impl std::error::Error for ClockSource {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

impl From<SystemTimeError> for super::CordialError {
    #[track_caller]
    #[instrument(level = "debug", skip(value))]
    fn from(value: SystemTimeError) -> Self {
        Self::from_kind(super::CordialErrorKind::Clock(ClockSource::from(value)))
    }
}
