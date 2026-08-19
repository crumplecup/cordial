//! Crate-local native sources (invariant, lookup, cargo metadata).

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::panic::Location;

use tracing::instrument;

#[derive(Debug)]
pub struct InvariantSource {
    message: String,
    location: &'static Location<'static>,
}

impl InvariantSource {
    #[track_caller]
    #[instrument(level = "debug", skip(message), ret)]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            location: Location::caller(),
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn location(&self) -> &'static Location<'static> {
        self.location
    }
}

impl Display for InvariantSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "invariant violated: {}", self.message)
    }
}

impl std::error::Error for InvariantSource {}

#[derive(Debug)]
pub struct UnknownEtiquetteSource {
    id: String,
    location: &'static Location<'static>,
}

impl UnknownEtiquetteSource {
    #[track_caller]
    #[instrument(level = "debug", skip(id), ret)]
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            location: Location::caller(),
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn location(&self) -> &'static Location<'static> {
        self.location
    }
}

impl Display for UnknownEtiquetteSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "etiquette not registered: {}", self.id)
    }
}

impl std::error::Error for UnknownEtiquetteSource {}

#[derive(Debug)]
pub struct CargoMetadataSource {
    source: cargo_metadata::Error,
    location: &'static Location<'static>,
}

impl CargoMetadataSource {
    #[track_caller]
    #[instrument(level = "debug", skip(source), ret)]
    pub fn new(source: cargo_metadata::Error) -> Self {
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

impl From<cargo_metadata::Error> for CargoMetadataSource {
    #[track_caller]
    #[instrument(level = "debug", skip(source), ret)]
    fn from(source: cargo_metadata::Error) -> Self {
        Self::new(source)
    }
}

impl Display for CargoMetadataSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "cargo metadata error: {}", self.source)
    }
}

impl std::error::Error for CargoMetadataSource {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

#[derive(Debug)]
pub struct NotFoundSource {
    path: std::path::PathBuf,
    location: &'static Location<'static>,
}

impl NotFoundSource {
    #[track_caller]
    #[instrument(level = "debug", skip(path), ret)]
    pub fn new(path: std::path::PathBuf) -> Self {
        Self {
            path,
            location: Location::caller(),
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn location(&self) -> &'static Location<'static> {
        self.location
    }
}

impl Display for NotFoundSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "not found: {}", self.path.display())
    }
}

impl std::error::Error for NotFoundSource {}

#[derive(Debug)]
pub struct NoExceptionsSource {
    etiquette: String,
    crate_name: String,
    location: &'static Location<'static>,
}

impl NoExceptionsSource {
    #[track_caller]
    #[instrument(level = "debug", skip(etiquette, crate_name), ret)]
    pub fn new(etiquette: impl Into<String>, crate_name: impl Into<String>) -> Self {
        Self {
            etiquette: etiquette.into(),
            crate_name: crate_name.into(),
            location: Location::caller(),
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn location(&self) -> &'static Location<'static> {
        self.location
    }
}

impl Display for NoExceptionsSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(
            formatter,
            "no exceptions for etiquette `{}` crate `{}`",
            self.etiquette, self.crate_name
        )
    }
}

impl std::error::Error for NoExceptionsSource {}

#[derive(Debug)]
pub struct NoCachedIrSource {
    path: std::path::PathBuf,
    location: &'static Location<'static>,
}

impl NoCachedIrSource {
    #[track_caller]
    #[instrument(level = "debug", skip(path), ret)]
    pub fn new(path: std::path::PathBuf) -> Self {
        Self {
            path,
            location: Location::caller(),
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn location(&self) -> &'static Location<'static> {
        self.location
    }
}

impl Display for NoCachedIrSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(
            formatter,
            "no cached IR at {} — run `cordial run` first",
            self.path.display()
        )
    }
}

impl std::error::Error for NoCachedIrSource {}
