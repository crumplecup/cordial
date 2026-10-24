//! Native sources for workspace-member/package lookups
//! (`targets.rs`, `etiquettes/antipatterns/version_in_member.rs`).

use std::panic::Location;
use std::path::PathBuf;

use tracing::instrument;

#[derive(Debug, derive_getters::Getters)]
pub struct MissingPackageMetadataSource {
    #[getter(skip)]
    package_id: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl MissingPackageMetadataSource {
    #[track_caller]
    #[instrument(level = "debug", skip(package_id))]
    pub fn new(package_id: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            package_id: package_id.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for MissingPackageMetadataSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "missing package metadata for {} ({}:{})",
            self.package_id, self.file, self.line
        )
    }
}

impl std::error::Error for MissingPackageMetadataSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct ManifestPathHasNoParentSource {
    #[getter(skip)]
    manifest_path: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl ManifestPathHasNoParentSource {
    #[track_caller]
    #[instrument(level = "debug", skip(manifest_path))]
    pub fn new(manifest_path: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            manifest_path: manifest_path.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for ManifestPathHasNoParentSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "manifest path has no parent: {} ({}:{})",
            self.manifest_path, self.file, self.line
        )
    }
}

impl std::error::Error for ManifestPathHasNoParentSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct NoWorkspaceMembersFoundSource {
    #[getter(skip)]
    project_root: PathBuf,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl NoWorkspaceMembersFoundSource {
    #[track_caller]
    #[instrument(level = "debug", skip(project_root))]
    pub fn new(project_root: PathBuf) -> Self {
        let loc = Location::caller();
        Self {
            project_root,
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for NoWorkspaceMembersFoundSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "no workspace members found at {} ({}:{})",
            self.project_root.display(),
            self.file,
            self.line
        )
    }
}

impl std::error::Error for NoWorkspaceMembersFoundSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct NoParentForManifestSource {
    #[getter(skip)]
    manifest_path: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl NoParentForManifestSource {
    #[track_caller]
    #[instrument(level = "debug", skip(manifest_path))]
    pub fn new(manifest_path: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            manifest_path: manifest_path.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for NoParentForManifestSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "no parent for {} ({}:{})",
            self.manifest_path, self.file, self.line
        )
    }
}

impl std::error::Error for NoParentForManifestSource {}

/// Umbrella kind for workspace-member/package lookup failures, nested
/// one level under [`super::CordialErrorKind`].
#[derive(Debug)]
pub enum TargetsErrorKind {
    MissingPackageMetadata(MissingPackageMetadataSource),
    ManifestPathHasNoParent(ManifestPathHasNoParentSource),
    NoWorkspaceMembersFound(NoWorkspaceMembersFoundSource),
    NoParentForManifest(NoParentForManifestSource),
}

#[derive(Debug)]
pub struct TargetsError {
    kind: Box<TargetsErrorKind>,
    file: String,
    line: u32,
}

impl TargetsError {
    #[track_caller]
    #[instrument(level = "debug", skip(kind))]
    fn new(kind: TargetsErrorKind) -> Self {
        let loc = Location::caller();
        Self {
            kind: Box::new(kind),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for TargetsError {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "[{}:{}] ", self.file, self.line)?;
        match self.kind.as_ref() {
            TargetsErrorKind::MissingPackageMetadata(source) => source.fmt(formatter),
            TargetsErrorKind::ManifestPathHasNoParent(source) => source.fmt(formatter),
            TargetsErrorKind::NoWorkspaceMembersFound(source) => source.fmt(formatter),
            TargetsErrorKind::NoParentForManifest(source) => source.fmt(formatter),
        }
    }
}

impl std::error::Error for TargetsError {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.kind.as_ref() {
            TargetsErrorKind::MissingPackageMetadata(source) => Some(source),
            TargetsErrorKind::ManifestPathHasNoParent(source) => Some(source),
            TargetsErrorKind::NoWorkspaceMembersFound(source) => Some(source),
            TargetsErrorKind::NoParentForManifest(source) => Some(source),
        }
    }
}

impl super::CordialError {
    /// `cargo metadata` did not include a workspace member's own package.
    #[track_caller]
    #[instrument(level = "debug", skip(package_id))]
    pub fn missing_package_metadata(package_id: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::Targets(TargetsError::new(
            TargetsErrorKind::MissingPackageMetadata(MissingPackageMetadataSource::new(package_id)),
        )))
    }

    /// A package manifest path had no parent directory.
    #[track_caller]
    #[instrument(level = "debug", skip(manifest_path))]
    pub fn manifest_path_has_no_parent(manifest_path: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::Targets(TargetsError::new(
            TargetsErrorKind::ManifestPathHasNoParent(ManifestPathHasNoParentSource::new(
                manifest_path,
            )),
        )))
    }

    /// No workspace members were found at a project root.
    #[track_caller]
    #[instrument(level = "debug", skip(project_root))]
    pub fn no_workspace_members_found(project_root: PathBuf) -> Self {
        Self::from_kind(super::CordialErrorKind::Targets(TargetsError::new(
            TargetsErrorKind::NoWorkspaceMembersFound(NoWorkspaceMembersFoundSource::new(
                project_root,
            )),
        )))
    }

    /// A package manifest path had no parent directory (antipatterns scan).
    #[track_caller]
    #[instrument(level = "debug", skip(manifest_path))]
    pub fn no_parent_for_manifest(manifest_path: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::Targets(TargetsError::new(
            TargetsErrorKind::NoParentForManifest(NoParentForManifestSource::new(manifest_path)),
        )))
    }
}
