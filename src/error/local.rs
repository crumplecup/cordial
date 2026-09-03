//! Crate-local native sources (invariant, unreachable, lookup, cargo metadata).

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::panic::Location;

use tracing::instrument;

#[derive(Debug, derive_getters::Getters)]
pub struct InvariantSource {
    #[getter(skip)]
    message: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl InvariantSource {
    #[track_caller]
    #[instrument(level = "debug", skip(message), ret)]
    pub fn new(message: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            message: message.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl Display for InvariantSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "invariant violated: {}", self.message)
    }
}

impl std::error::Error for InvariantSource {}

/// A match arm (or equivalent control path) that must not execute.
///
/// Prefer making every arm of a sum type meaningful. When a path is still
/// required by the type but is not a valid outcome, return this instead of
/// aborting so callers keep control through error handling.
#[derive(Debug, derive_getters::Getters)]
pub struct UnreachableSource {
    #[getter(skip)]
    message: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl UnreachableSource {
    #[track_caller]
    #[instrument(level = "debug", skip(message), ret)]
    pub fn new(message: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            message: message.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl Display for UnreachableSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(
            formatter,
            "unreachable arm at {}:{}: {}",
            self.file, self.line, self.message
        )
    }
}

impl std::error::Error for UnreachableSource {}

/// A [`derive_builder`] `build()` that left a field unset.
///
/// `build()` returns [`derive_builder::UninitializedFieldError`]. Wrap it
/// here so callers keep file and line on the error and can `?` into
/// [`crate::CordialError`] instead of aborting.
#[derive(Debug, derive_getters::Getters)]
pub struct BuilderSource {
    #[getter(skip)]
    source: derive_builder::UninitializedFieldError,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl BuilderSource {
    #[track_caller]
    #[instrument(level = "debug", skip(source), ret)]
    pub fn new(source: derive_builder::UninitializedFieldError) -> Self {
        let loc = Location::caller();
        Self {
            source,
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl From<derive_builder::UninitializedFieldError> for BuilderSource {
    #[track_caller]
    #[instrument(level = "debug", skip(source), ret)]
    fn from(source: derive_builder::UninitializedFieldError) -> Self {
        Self::new(source)
    }
}

impl Display for BuilderSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(
            formatter,
            "builder failed at {}:{}: {}",
            self.file, self.line, self.source
        )
    }
}

impl std::error::Error for BuilderSource {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

#[derive(Debug, derive_getters::Getters)]
pub struct UnknownEtiquetteSource {
    #[getter(skip)]
    id: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl UnknownEtiquetteSource {
    #[track_caller]
    #[instrument(level = "debug", skip(id), ret)]
    pub fn new(id: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            id: id.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl Display for UnknownEtiquetteSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "etiquette not registered: {}", self.id)
    }
}

impl std::error::Error for UnknownEtiquetteSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct CargoMetadataSource {
    #[getter(skip)]
    source: cargo_metadata::Error,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl CargoMetadataSource {
    #[track_caller]
    #[instrument(level = "debug", skip(source), ret)]
    pub fn new(source: cargo_metadata::Error) -> Self {
        let loc = Location::caller();
        Self {
            source,
            file: loc.file().to_string(),
            line: loc.line(),
        }
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

#[derive(Debug, derive_getters::Getters)]
pub struct NotFoundSource {
    #[getter(skip)]
    path: std::path::PathBuf,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl NotFoundSource {
    #[track_caller]
    #[instrument(level = "debug", skip(path), ret)]
    pub fn new(path: std::path::PathBuf) -> Self {
        let loc = Location::caller();
        Self {
            path,
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl Display for NotFoundSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "not found: {}", self.path.display())
    }
}

impl std::error::Error for NotFoundSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct NoExceptionsSource {
    #[getter(skip)]
    etiquette: String,
    #[getter(skip)]
    crate_name: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl NoExceptionsSource {
    #[track_caller]
    #[instrument(level = "debug", skip(etiquette, crate_name), ret)]
    pub fn new(etiquette: impl Into<String>, crate_name: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            etiquette: etiquette.into(),
            crate_name: crate_name.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
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

#[derive(Debug, derive_getters::Getters)]
pub struct NoCachedIrSource {
    #[getter(skip)]
    path: std::path::PathBuf,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl NoCachedIrSource {
    #[track_caller]
    #[instrument(level = "debug", skip(path), ret)]
    pub fn new(path: std::path::PathBuf) -> Self {
        let loc = Location::caller();
        Self {
            path,
            file: loc.file().to_string(),
            line: loc.line(),
        }
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
