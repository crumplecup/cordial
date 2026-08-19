//! Parse-family native sources (JSON, config, syn, token stream).

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::panic::Location;

use tracing::instrument;

#[derive(Debug)]
pub struct JsonSource {
    source: serde_json::Error,
    location: &'static Location<'static>,
}

impl JsonSource {
    #[track_caller]
    #[instrument(level = "debug", skip(source), ret)]
    pub fn new(source: serde_json::Error) -> Self {
        Self {
            source,
            location: Location::caller(),
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn location(&self) -> &'static Location<'static> {
        self.location
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

#[derive(Debug)]
pub struct JsonParseSource {
    source: serde_json::Error,
    path: String,
    location: &'static Location<'static>,
}

impl JsonParseSource {
    #[track_caller]
    #[instrument(level = "debug", skip(path, source), ret)]
    pub fn new(path: impl Into<String>, source: serde_json::Error) -> Self {
        Self {
            source,
            path: path.into(),
            location: Location::caller(),
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn location(&self) -> &'static Location<'static> {
        self.location
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

#[derive(Debug)]
pub struct ConfigSource {
    source: config::ConfigError,
    location: &'static Location<'static>,
}

impl ConfigSource {
    #[track_caller]
    #[instrument(level = "debug", skip(source), ret)]
    pub fn new(source: config::ConfigError) -> Self {
        Self {
            source,
            location: Location::caller(),
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn location(&self) -> &'static Location<'static> {
        self.location
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

#[derive(Debug)]
pub struct SynParseSource {
    source: syn::Error,
    path: String,
    location: &'static Location<'static>,
}

impl SynParseSource {
    #[track_caller]
    #[instrument(level = "debug", skip(path, source), ret)]
    pub fn new(path: impl Into<String>, source: syn::Error) -> Self {
        Self {
            source,
            path: path.into(),
            location: Location::caller(),
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn location(&self) -> &'static Location<'static> {
        self.location
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
#[derive(Debug)]
pub struct TokenStreamParseError {
    source: String,
    location: &'static Location<'static>,
}

impl TokenStreamParseError {
    #[track_caller]
    #[instrument(level = "debug", skip(source), ret)]
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            location: Location::caller(),
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn location(&self) -> &'static Location<'static> {
        self.location
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
