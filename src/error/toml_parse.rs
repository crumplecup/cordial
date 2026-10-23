//! TOML parsing errors, available when an etiquette reads TOML files.

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::panic::Location;

use tracing::instrument;

use super::{CordialError, CordialErrorKind};

#[derive(Debug, derive_getters::Getters)]
pub struct TomlParseSource {
    #[getter(skip)]
    source: toml::de::Error,
    #[getter(skip)]
    path: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl TomlParseSource {
    #[track_caller]
    #[instrument(level = "debug", skip(path, source))]
    pub fn new(path: impl Into<String>, source: toml::de::Error) -> Self {
        let loc = Location::caller();
        Self {
            source,
            path: path.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl Display for TomlParseSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(
            formatter,
            "TOML parse error in {}: {}",
            self.path, self.source
        )
    }
}

impl std::error::Error for TomlParseSource {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

impl CordialError {
    /// TOML failed to parse at this path.
    #[track_caller]
    #[instrument(level = "debug", skip(path, err))]
    pub fn toml_parse(path: impl Into<String>, err: toml::de::Error) -> Self {
        Self::from_kind(CordialErrorKind::TomlParse(TomlParseSource::new(path, err)))
    }
}
