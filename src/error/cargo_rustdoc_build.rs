//! Native sources for `cargo_rustdoc`/`cargo doc` build subprocesses,
//! and the `amenable dump-registry` subprocess that shares their
//! exit-status/JSON-not-found shape.

use std::panic::Location;
use std::path::PathBuf;
use std::process::ExitStatus;

use tracing::instrument;

#[derive(Debug, derive_getters::Getters)]
pub struct AmenableDumpRegistryExitedSource {
    #[getter(copy)]
    status: ExitStatus,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl AmenableDumpRegistryExitedSource {
    #[track_caller]
    #[instrument(level = "debug", skip(status))]
    pub fn new(status: ExitStatus) -> Self {
        let loc = Location::caller();
        Self {
            status,
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for AmenableDumpRegistryExitedSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "amenable dump-registry exited with {} ({}:{})",
            self.status, self.file, self.line
        )
    }
}

impl std::error::Error for AmenableDumpRegistryExitedSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct AmenableDumpRegistryDidNotWriteSource {
    #[getter(skip)]
    path: PathBuf,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl AmenableDumpRegistryDidNotWriteSource {
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

impl std::fmt::Display for AmenableDumpRegistryDidNotWriteSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "amenable dump-registry did not write {} ({}:{})",
            self.path.display(),
            self.file,
            self.line
        )
    }
}

impl std::error::Error for AmenableDumpRegistryDidNotWriteSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct CargoRustdocExitedSource {
    #[getter(skip)]
    crate_name: String,
    #[getter(copy)]
    status: ExitStatus,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl CargoRustdocExitedSource {
    #[track_caller]
    #[instrument(level = "debug", skip(crate_name, status))]
    pub fn new(crate_name: impl Into<String>, status: ExitStatus) -> Self {
        let loc = Location::caller();
        Self {
            crate_name: crate_name.into(),
            status,
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for CargoRustdocExitedSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "cargo rustdoc for {} exited with {} ({}:{})",
            self.crate_name, self.status, self.file, self.line
        )
    }
}

impl std::error::Error for CargoRustdocExitedSource {}

/// Shared by every `cargo rustdoc`/`cargo doc` build path -- the
/// expected rustdoc JSON output file never appeared.
#[derive(Debug, derive_getters::Getters)]
pub struct RustdocJsonNotFoundSource {
    #[getter(skip)]
    path: PathBuf,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl RustdocJsonNotFoundSource {
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

impl std::fmt::Display for RustdocJsonNotFoundSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "rustdoc JSON not found at {} ({}:{})",
            self.path.display(),
            self.file,
            self.line
        )
    }
}

impl std::error::Error for RustdocJsonNotFoundSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct CargoDocForOptionalDepExitedSource {
    #[getter(skip)]
    shadow_crate: String,
    #[getter(skip)]
    activating_feature: String,
    #[getter(copy)]
    status: ExitStatus,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl CargoDocForOptionalDepExitedSource {
    #[track_caller]
    #[instrument(level = "debug", skip(shadow_crate, activating_feature, status))]
    pub fn new(
        shadow_crate: impl Into<String>,
        activating_feature: impl Into<String>,
        status: ExitStatus,
    ) -> Self {
        let loc = Location::caller();
        Self {
            shadow_crate: shadow_crate.into(),
            activating_feature: activating_feature.into(),
            status,
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for CargoDocForOptionalDepExitedSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "cargo doc for {} (features={}) exited with {} ({}:{})",
            self.shadow_crate, self.activating_feature, self.status, self.file, self.line
        )
    }
}

impl std::error::Error for CargoDocForOptionalDepExitedSource {}

/// Umbrella kind for `cargo rustdoc`/`cargo doc` build failures, nested
/// one level under [`super::CordialErrorKind`] so the top-level enum
/// gets one variant per domain instead of one per failure.
#[derive(Debug)]
pub enum CargoRustdocBuildErrorKind {
    AmenableDumpRegistryExited(AmenableDumpRegistryExitedSource),
    AmenableDumpRegistryDidNotWrite(AmenableDumpRegistryDidNotWriteSource),
    CargoRustdocExited(CargoRustdocExitedSource),
    RustdocJsonNotFound(RustdocJsonNotFoundSource),
    CargoDocForOptionalDepExited(CargoDocForOptionalDepExitedSource),
}

#[derive(Debug)]
pub struct CargoRustdocBuildError {
    kind: Box<CargoRustdocBuildErrorKind>,
    file: String,
    line: u32,
}

impl CargoRustdocBuildError {
    #[track_caller]
    #[instrument(level = "debug", skip(kind))]
    fn new(kind: CargoRustdocBuildErrorKind) -> Self {
        let loc = Location::caller();
        Self {
            kind: Box::new(kind),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for CargoRustdocBuildError {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "[{}:{}] ", self.file, self.line)?;
        match self.kind.as_ref() {
            CargoRustdocBuildErrorKind::AmenableDumpRegistryExited(source) => source.fmt(formatter),
            CargoRustdocBuildErrorKind::AmenableDumpRegistryDidNotWrite(source) => {
                source.fmt(formatter)
            }
            CargoRustdocBuildErrorKind::CargoRustdocExited(source) => source.fmt(formatter),
            CargoRustdocBuildErrorKind::RustdocJsonNotFound(source) => source.fmt(formatter),
            CargoRustdocBuildErrorKind::CargoDocForOptionalDepExited(source) => {
                source.fmt(formatter)
            }
        }
    }
}

impl std::error::Error for CargoRustdocBuildError {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.kind.as_ref() {
            CargoRustdocBuildErrorKind::AmenableDumpRegistryExited(source) => Some(source),
            CargoRustdocBuildErrorKind::AmenableDumpRegistryDidNotWrite(source) => Some(source),
            CargoRustdocBuildErrorKind::CargoRustdocExited(source) => Some(source),
            CargoRustdocBuildErrorKind::RustdocJsonNotFound(source) => Some(source),
            CargoRustdocBuildErrorKind::CargoDocForOptionalDepExited(source) => Some(source),
        }
    }
}

impl super::CordialError {
    /// `amenable dump-registry` exited with a non-zero status.
    #[track_caller]
    #[instrument(level = "debug", skip(status))]
    pub fn amenable_dump_registry_exited(status: ExitStatus) -> Self {
        Self::from_kind(super::CordialErrorKind::CargoRustdocBuild(
            CargoRustdocBuildError::new(CargoRustdocBuildErrorKind::AmenableDumpRegistryExited(
                AmenableDumpRegistryExitedSource::new(status),
            )),
        ))
    }

    /// `amenable dump-registry` exited successfully but did not write its output.
    #[track_caller]
    #[instrument(level = "debug", skip(path))]
    pub fn amenable_dump_registry_did_not_write(path: PathBuf) -> Self {
        Self::from_kind(super::CordialErrorKind::CargoRustdocBuild(
            CargoRustdocBuildError::new(
                CargoRustdocBuildErrorKind::AmenableDumpRegistryDidNotWrite(
                    AmenableDumpRegistryDidNotWriteSource::new(path),
                ),
            ),
        ))
    }

    /// `cargo rustdoc` exited with a non-zero status.
    #[track_caller]
    #[instrument(level = "debug", skip(crate_name, status))]
    pub fn cargo_rustdoc_exited(crate_name: impl Into<String>, status: ExitStatus) -> Self {
        Self::from_kind(super::CordialErrorKind::CargoRustdocBuild(
            CargoRustdocBuildError::new(CargoRustdocBuildErrorKind::CargoRustdocExited(
                CargoRustdocExitedSource::new(crate_name, status),
            )),
        ))
    }

    /// A `cargo rustdoc`/`cargo doc` build did not produce the expected JSON file.
    #[track_caller]
    #[instrument(level = "debug", skip(path))]
    pub fn rustdoc_json_not_found(path: PathBuf) -> Self {
        Self::from_kind(super::CordialErrorKind::CargoRustdocBuild(
            CargoRustdocBuildError::new(CargoRustdocBuildErrorKind::RustdocJsonNotFound(
                RustdocJsonNotFoundSource::new(path),
            )),
        ))
    }

    /// `cargo doc` for an optional dependency's shadow crate exited with a non-zero status.
    #[track_caller]
    #[instrument(level = "debug", skip(shadow_crate, activating_feature, status))]
    pub fn cargo_doc_for_optional_dep_exited(
        shadow_crate: impl Into<String>,
        activating_feature: impl Into<String>,
        status: ExitStatus,
    ) -> Self {
        Self::from_kind(super::CordialErrorKind::CargoRustdocBuild(
            CargoRustdocBuildError::new(CargoRustdocBuildErrorKind::CargoDocForOptionalDepExited(
                CargoDocForOptionalDepExitedSource::new(shadow_crate, activating_feature, status),
            )),
        ))
    }
}
