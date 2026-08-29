//! Parse-family native sources (JSON, config, syn, token stream).

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
    #[instrument(level = "debug", skip(source), ret)]
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
    fn from(source: serde_json::Error) -> Self {
        Self::new(source)
    }
}

impl Display for JsonSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "JSON error: {}", self.source)
    }
}

impl std::error::Error for JsonSource {
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
    #[instrument(level = "debug", skip(path, source), ret)]
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
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "JSON error in {}: {}", self.path, self.source)
    }
}

impl std::error::Error for JsonParseSource {
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
    #[instrument(level = "debug", skip(source), ret)]
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
    fn from(source: config::ConfigError) -> Self {
        Self::new(source)
    }
}

impl Display for ConfigSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "config error: {}", self.source)
    }
}

impl std::error::Error for ConfigSource {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

#[derive(Debug, derive_getters::Getters)]
pub struct SynParseSource {
    #[getter(skip)]
    source: syn::Error,
    #[getter(skip)]
    path: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl SynParseSource {
    #[track_caller]
    #[instrument(level = "debug", skip(path, source), ret)]
    pub fn new(path: impl Into<String>, source: syn::Error) -> Self {
        let loc = Location::caller();
        Self {
            source,
            path: path.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl Display for SynParseSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(
            formatter,
            "syn parse error in {}: {}",
            self.path, self.source
        )
    }
}

impl std::error::Error for SynParseSource {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Send/Sync stand-in for [`proc_macro2::LexError`], which is not Send.
#[derive(Debug, derive_getters::Getters)]
pub struct TokenStreamParseError {
    #[getter(skip)]
    source: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl TokenStreamParseError {
    /// Construct a new value.
    #[track_caller]
    #[instrument(level = "debug", skip(source), ret)]
    pub fn new(source: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            source: source.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl From<proc_macro2::LexError> for TokenStreamParseError {
    #[track_caller]
    #[instrument(level = "debug", skip(source), ret)]
    fn from(source: proc_macro2::LexError) -> Self {
        Self::new(source.to_string())
    }
}

impl Display for TokenStreamParseError {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "token stream parse error: {}", self.source)
    }
}

impl std::error::Error for TokenStreamParseError {}
