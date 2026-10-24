//! Crate-local native sources (invariant, unreachable, lookup, cargo metadata).

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::panic::Location;
use std::path::PathBuf;

use tracing::instrument;

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
    #[instrument(level = "debug", skip(message))]
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
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(
            formatter,
            "unreachable arm at {}:{}: {}",
            self.file, self.line, self.message
        )
    }
}

impl std::error::Error for UnreachableSource {}

/// `apply_gap` was called for `TracingApplyPolicy::Skip`, which
/// `strip_instrument` handles instead -- `apply_gap` only ever runs for
/// the write path.
#[derive(Debug, derive_getters::Getters)]
pub struct ApplyGapWriteOnlySource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl ApplyGapWriteOnlySource {
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn new() -> Self {
        let loc = Location::caller();
        Self {
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl Display for ApplyGapWriteOnlySource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(
            formatter,
            "apply_gap is the write path; Skip is handled by strip_instrument"
        )
    }
}

impl std::error::Error for ApplyGapWriteOnlySource {}

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
    #[instrument(level = "debug", skip(source))]
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
    #[instrument(level = "debug", skip(source))]
    fn from(source: derive_builder::UninitializedFieldError) -> Self {
        Self::new(source)
    }
}

impl Display for BuilderSource {
    #[instrument(level = "trace", skip(self, formatter))]
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
    #[instrument(level = "debug", skip(id))]
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
    #[instrument(level = "trace", skip(self, formatter))]
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
    #[instrument(level = "debug", skip(source))]
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
    #[instrument(level = "debug", skip(source))]
    fn from(source: cargo_metadata::Error) -> Self {
        Self::new(source)
    }
}

impl Display for CargoMetadataSource {
    #[instrument(level = "trace", skip(self, formatter))]
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
    #[instrument(level = "debug", skip(path))]
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
    #[instrument(level = "trace", skip(self, formatter))]
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
    #[instrument(level = "debug", skip(etiquette, crate_name))]
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
    #[instrument(level = "trace", skip(self, formatter))]
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
    path: PathBuf,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl NoCachedIrSource {
    #[track_caller]
    #[instrument(level = "debug", skip(path))]
    pub fn new(path: PathBuf) -> Self {
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

#[derive(Debug, derive_getters::Getters)]
pub struct MissingRustdocJsonSource {
    #[getter(skip)]
    crate_name: String,
    #[getter(skip)]
    crate_root: PathBuf,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl MissingRustdocJsonSource {
    #[track_caller]
    #[instrument(level = "debug", skip(crate_name, crate_root))]
    pub fn new(crate_name: impl Into<String>, crate_root: PathBuf) -> Self {
        let loc = Location::caller();
        Self {
            crate_name: crate_name.into(),
            crate_root,
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl Display for MissingRustdocJsonSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(
            formatter,
            "rustdoc JSON not found for crate `{}` under {}; automatic rustdoc cache rebuild did not produce it",
            self.crate_name,
            self.crate_root.display()
        )
    }
}

impl std::error::Error for MissingRustdocJsonSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct OpenFindingsSource {
    #[getter(copy)]
    count: usize,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl OpenFindingsSource {
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn new(count: usize) -> Self {
        let loc = Location::caller();
        Self {
            count,
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl Display for OpenFindingsSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(
            formatter,
            "open action items denied: {} open action items remain",
            self.count
        )
    }
}

impl std::error::Error for OpenFindingsSource {}

impl super::CordialError {
    /// A match arm (or equivalent control path) that must not execute.
    ///
    /// Use this instead of `unreachable!` / `panic!` so the location stays on
    /// the error and callers can keep control through `?`.
    #[track_caller]
    #[instrument(level = "debug", skip(message))]
    pub fn unreachable(message: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::Unreachable(
            UnreachableSource::new(message),
        ))
    }

    /// `apply_gap` was called for `TracingApplyPolicy::Skip`.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn apply_gap_write_only() -> Self {
        Self::from_kind(super::CordialErrorKind::ApplyGapWriteOnly(
            ApplyGapWriteOnlySource::new(),
        ))
    }

    /// A [`derive_builder`] `build()` left a required field unset.
    #[track_caller]
    #[instrument(level = "debug", skip(source))]
    pub fn builder(source: derive_builder::UninitializedFieldError) -> Self {
        Self::from(source)
    }

    /// No etiquette is registered under this id.
    #[track_caller]
    #[instrument(level = "debug", skip(id))]
    pub fn unknown_etiquette(id: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::UnknownEtiquette(
            UnknownEtiquetteSource::new(id),
        ))
    }

    /// Wrap a `cargo_metadata` failure.
    #[track_caller]
    #[instrument(level = "debug", skip(err))]
    pub fn cargo_metadata(err: cargo_metadata::Error) -> Self {
        Self::from(err)
    }

    /// This path does not exist.
    #[track_caller]
    #[instrument(level = "debug", skip(path))]
    pub fn not_found(path: PathBuf) -> Self {
        Self::from_kind(super::CordialErrorKind::NotFound(NotFoundSource::new(path)))
    }

    /// No exception file exists for this etiquette and crate.
    #[track_caller]
    #[instrument(level = "debug", skip(etiquette, crate_name))]
    pub fn no_exceptions(etiquette: impl Into<String>, crate_name: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::NoExceptions(
            NoExceptionsSource::new(etiquette, crate_name),
        ))
    }

    /// No cached IR artifact exists at this path.
    #[track_caller]
    #[instrument(level = "debug", skip(path))]
    pub fn no_cached_ir(path: PathBuf) -> Self {
        Self::from_kind(super::CordialErrorKind::NoCachedIr(NoCachedIrSource::new(
            path,
        )))
    }

    /// Rustdoc JSON is required for this crate but has not been built.
    #[track_caller]
    #[instrument(level = "debug", skip(crate_root, crate_name))]
    pub fn missing_rustdoc_json(crate_name: impl Into<String>, crate_root: PathBuf) -> Self {
        Self::from_kind(super::CordialErrorKind::MissingRustdocJson(
            MissingRustdocJsonSource::new(crate_name, crate_root),
        ))
    }

    /// Open action items were denied by a CLI gate.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn open_findings(count: usize) -> Self {
        Self::from_kind(super::CordialErrorKind::OpenFindings(
            OpenFindingsSource::new(count),
        ))
    }
}

impl From<cargo_metadata::Error> for super::CordialError {
    #[track_caller]
    fn from(value: cargo_metadata::Error) -> Self {
        Self::from_kind(super::CordialErrorKind::CargoMetadata(
            CargoMetadataSource::from(value),
        ))
    }
}

impl From<derive_builder::UninitializedFieldError> for super::CordialError {
    #[track_caller]
    #[instrument(level = "debug", skip(value))]
    fn from(value: derive_builder::UninitializedFieldError) -> Self {
        Self::from_kind(super::CordialErrorKind::Builder(BuilderSource::from(value)))
    }
}
