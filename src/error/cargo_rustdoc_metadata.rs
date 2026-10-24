//! Native sources for workspace/dependency `cargo metadata` lookups
//! used by impl-coverage feature probes.

use std::panic::Location;

use tracing::instrument;

#[derive(Debug, derive_getters::Getters)]
pub struct WorkspacePackageNotFoundSource {
    #[getter(skip)]
    member_crate_name: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl WorkspacePackageNotFoundSource {
    #[track_caller]
    #[instrument(level = "debug", skip(member_crate_name))]
    pub fn new(member_crate_name: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            member_crate_name: member_crate_name.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for WorkspacePackageNotFoundSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "workspace package `{}` not found in cargo metadata ({}:{})",
            self.member_crate_name, self.file, self.line
        )
    }
}

impl std::error::Error for WorkspacePackageNotFoundSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct DependencyNotFoundInPackageSource {
    #[getter(skip)]
    crate_name: String,
    #[getter(skip)]
    member_crate_name: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl DependencyNotFoundInPackageSource {
    #[track_caller]
    #[instrument(level = "debug", skip(crate_name, member_crate_name))]
    pub fn new(crate_name: impl Into<String>, member_crate_name: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            crate_name: crate_name.into(),
            member_crate_name: member_crate_name.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for DependencyNotFoundInPackageSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "dependency '{}' not found in `{}` package metadata ({}:{})",
            self.crate_name, self.member_crate_name, self.file, self.line
        )
    }
}

impl std::error::Error for DependencyNotFoundInPackageSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct PackageNotFoundInWorkspaceSource {
    #[getter(skip)]
    crate_name: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl PackageNotFoundInWorkspaceSource {
    #[track_caller]
    #[instrument(level = "debug", skip(crate_name))]
    pub fn new(crate_name: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            crate_name: crate_name.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for PackageNotFoundInWorkspaceSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "package '{}' not found in workspace metadata ({}:{})",
            self.crate_name, self.file, self.line
        )
    }
}

impl std::error::Error for PackageNotFoundInWorkspaceSource {}

/// Umbrella kind for `cargo metadata` lookup failures, nested one
/// level under [`super::CordialErrorKind`].
#[derive(Debug)]
pub enum CargoRustdocMetadataErrorKind {
    WorkspacePackageNotFound(WorkspacePackageNotFoundSource),
    DependencyNotFoundInPackage(DependencyNotFoundInPackageSource),
    PackageNotFoundInWorkspace(PackageNotFoundInWorkspaceSource),
}

#[derive(Debug)]
pub struct CargoRustdocMetadataError {
    kind: Box<CargoRustdocMetadataErrorKind>,
    file: String,
    line: u32,
}

impl CargoRustdocMetadataError {
    #[track_caller]
    #[instrument(level = "debug", skip(kind))]
    fn new(kind: CargoRustdocMetadataErrorKind) -> Self {
        let loc = Location::caller();
        Self {
            kind: Box::new(kind),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for CargoRustdocMetadataError {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "[{}:{}] ", self.file, self.line)?;
        match self.kind.as_ref() {
            CargoRustdocMetadataErrorKind::WorkspacePackageNotFound(source) => {
                source.fmt(formatter)
            }
            CargoRustdocMetadataErrorKind::DependencyNotFoundInPackage(source) => {
                source.fmt(formatter)
            }
            CargoRustdocMetadataErrorKind::PackageNotFoundInWorkspace(source) => {
                source.fmt(formatter)
            }
        }
    }
}

impl std::error::Error for CargoRustdocMetadataError {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.kind.as_ref() {
            CargoRustdocMetadataErrorKind::WorkspacePackageNotFound(source) => Some(source),
            CargoRustdocMetadataErrorKind::DependencyNotFoundInPackage(source) => Some(source),
            CargoRustdocMetadataErrorKind::PackageNotFoundInWorkspace(source) => Some(source),
        }
    }
}

impl super::CordialError {
    /// A workspace member package was not found in `cargo metadata`.
    #[track_caller]
    #[instrument(level = "debug", skip(member_crate_name))]
    pub fn workspace_package_not_found(member_crate_name: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::CargoRustdocMetadata(
            CargoRustdocMetadataError::new(
                CargoRustdocMetadataErrorKind::WorkspacePackageNotFound(
                    WorkspacePackageNotFoundSource::new(member_crate_name),
                ),
            ),
        ))
    }

    /// A dependency was not found in a workspace member's package metadata.
    #[track_caller]
    #[instrument(level = "debug", skip(crate_name, member_crate_name))]
    pub fn dependency_not_found_in_package(
        crate_name: impl Into<String>,
        member_crate_name: impl Into<String>,
    ) -> Self {
        Self::from_kind(super::CordialErrorKind::CargoRustdocMetadata(
            CargoRustdocMetadataError::new(
                CargoRustdocMetadataErrorKind::DependencyNotFoundInPackage(
                    DependencyNotFoundInPackageSource::new(crate_name, member_crate_name),
                ),
            ),
        ))
    }

    /// A package was not found in workspace metadata.
    #[track_caller]
    #[instrument(level = "debug", skip(crate_name))]
    pub fn package_not_found_in_workspace(crate_name: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::CargoRustdocMetadata(
            CargoRustdocMetadataError::new(
                CargoRustdocMetadataErrorKind::PackageNotFoundInWorkspace(
                    PackageNotFoundInWorkspaceSource::new(crate_name),
                ),
            ),
        ))
    }
}
