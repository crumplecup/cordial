//! Preserve the Cargo loader's opaque upstream error without stringifying it.

use std::panic::Location;
use tracing::instrument;

/// rust-analyzer's workspace loader failed before semantic analysis began.
#[derive(Debug, derive_getters::Getters)]
pub struct SemanticLoadSource {
    #[getter(skip)]
    source: anyhow::Error,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl SemanticLoadSource {
    #[track_caller]
    #[instrument(level = "debug", skip(source))]
    fn new(source: anyhow::Error) -> Self {
        let location = Location::caller();
        Self {
            source,
            file: location.file().to_owned(),
            line: location.line(),
        }
    }
}

impl From<anyhow::Error> for super::CordialError {
    #[track_caller]
    #[instrument(level = "debug", skip(source))]
    fn from(source: anyhow::Error) -> Self {
        Self::from_kind(super::CordialErrorKind::SemanticLoad(
            SemanticLoadSource::new(source),
        ))
    }
}

impl std::fmt::Display for SemanticLoadSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "semantic workspace loading failed at {}:{}: {}",
            self.file, self.line, self.source
        )
    }
}

impl std::error::Error for SemanticLoadSource {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

/// A source revision cannot rebuild a frozen Cargo input or compiled macro.
#[derive(Debug, derive_getters::Getters)]
pub struct SemanticContextInputSource {
    path: std::path::PathBuf,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl SemanticContextInputSource {
    #[track_caller]
    #[instrument(level = "debug", skip(path))]
    fn new(path: std::path::PathBuf) -> Self {
        let location = Location::caller();
        Self {
            path,
            file: location.file().to_owned(),
            line: location.line(),
        }
    }
}

impl super::CordialError {
    /// Reload the workspace driver before changing this Cargo context input.
    #[track_caller]
    #[instrument(level = "debug", skip(path))]
    pub fn semantic_context_input(path: std::path::PathBuf) -> Self {
        Self::from_kind(super::CordialErrorKind::SemanticContextInput(
            SemanticContextInputSource::new(path),
        ))
    }
}

impl std::fmt::Display for SemanticContextInputSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "semantic Cargo context requires a workspace reload: {} ({}:{})",
            self.path.display(),
            self.file,
            self.line
        )
    }
}

impl std::error::Error for SemanticContextInputSource {}
