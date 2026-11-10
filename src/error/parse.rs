//! Parse-family native sources (JSON and config).

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::panic::Location;

use tracing::instrument;

#[derive(Debug, derive_getters::Getters)]
pub struct JsonSource {
    #[getter(skip)]
    source: serde_json::Error,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl JsonSource {
    #[track_caller]
    #[instrument(level = "debug", skip(source))]
    pub fn new(source: serde_json::Error) -> Self {
        let loc = Location::caller();
        Self {
            source,
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl From<serde_json::Error> for JsonSource {
    #[track_caller]
    #[instrument(level = "debug", skip(source))]
    fn from(source: serde_json::Error) -> Self {
        Self::new(source)
    }
}

impl Display for JsonSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "JSON error: {}", self.source)
    }
}

impl std::error::Error for JsonSource {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

#[derive(Debug, derive_getters::Getters)]
pub struct JsonParseSource {
    #[getter(skip)]
    source: serde_json::Error,
    #[getter(skip)]
    path: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl JsonParseSource {
    #[track_caller]
    #[instrument(level = "debug", skip(path, source))]
    pub fn new(path: impl Into<String>, source: serde_json::Error) -> Self {
        let loc = Location::caller();
        Self {
            source,
            path: path.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl Display for JsonParseSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "JSON error in {}: {}", self.path, self.source)
    }
}

impl std::error::Error for JsonParseSource {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

#[derive(Debug, derive_getters::Getters)]
pub struct ConfigSource {
    #[getter(skip)]
    source: config::ConfigError,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl ConfigSource {
    #[track_caller]
    #[instrument(level = "debug", skip(source))]
    pub fn new(source: config::ConfigError) -> Self {
        let loc = Location::caller();
        Self {
            source,
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl From<config::ConfigError> for ConfigSource {
    #[track_caller]
    #[instrument(level = "debug", skip(source))]
    fn from(source: config::ConfigError) -> Self {
        Self::new(source)
    }
}

impl Display for ConfigSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "config error: {}", self.source)
    }
}

impl std::error::Error for ConfigSource {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

impl super::CordialError {
    /// JSON failed to parse at this path.
    #[track_caller]
    #[instrument(level = "debug", skip(path, err))]
    pub fn json_parse(path: impl Into<String>, err: serde_json::Error) -> Self {
        Self::from_kind(super::CordialErrorKind::JsonParse(JsonParseSource::new(
            path, err,
        )))
    }
}

impl From<serde_json::Error> for super::CordialError {
    #[track_caller]
    #[instrument(level = "debug", skip(value))]
    fn from(value: serde_json::Error) -> Self {
        Self::from_kind(super::CordialErrorKind::Json(JsonSource::from(value)))
    }
}

impl From<config::ConfigError> for super::CordialError {
    #[track_caller]
    #[instrument(level = "debug", skip(value))]
    fn from(value: config::ConfigError) -> Self {
        Self::from_kind(super::CordialErrorKind::Config(ConfigSource::from(value)))
    }
}
