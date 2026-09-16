use std::fmt::{Display, Formatter, Result as FmtResult};

use tracing::instrument;

/// Cargo dependency section that declared a dependency.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DependencySection {
    /// `[dependencies]`.
    Normal,
    /// `[dev-dependencies]`.
    Dev,
    /// `[build-dependencies]`.
    Build,
    /// `[target.'cfg(...)'.dependencies]`.
    TargetNormal(String),
    /// `[target.'cfg(...)'.dev-dependencies]`.
    TargetDev(String),
    /// `[target.'cfg(...)'.build-dependencies]`.
    TargetBuild(String),
    /// `[workspace.dependencies]`.
    Workspace,
}

impl Display for DependencySection {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Normal => formatter.write_str("dependencies"),
            Self::Dev => formatter.write_str("dev-dependencies"),
            Self::Build => formatter.write_str("build-dependencies"),
            Self::TargetNormal(target) => write!(formatter, "target.{target}.dependencies"),
            Self::TargetDev(target) => write!(formatter, "target.{target}.dev-dependencies"),
            Self::TargetBuild(target) => write!(formatter, "target.{target}.build-dependencies"),
            Self::Workspace => formatter.write_str("workspace.dependencies"),
        }
    }
}

/// Where a dependency is sourced from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DependencySourceKind {
    /// crates.io or another registry source.
    Registry,
    /// Filesystem path dependency.
    Path,
    /// Git dependency.
    Git,
    /// Member manifest inherits from `[workspace.dependencies]`.
    Workspace,
    /// No source was explicit in the manifest entry.
    Unspecified,
}

impl Display for DependencySourceKind {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Registry => formatter.write_str("registry"),
            Self::Path => formatter.write_str("path"),
            Self::Git => formatter.write_str("git"),
            Self::Workspace => formatter.write_str("workspace"),
            Self::Unspecified => formatter.write_str("unspecified"),
        }
    }
}

/// Version intent visible in `Cargo.toml`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ManifestVersionSpec {
    /// Version requirement string such as `1`, `~1.2`, or `=1.2.3`.
    Requirement(String),
    /// Member dependency uses `workspace = true`, optionally with the inherited requirement.
    WorkspaceInherited(Option<String>),
    /// Entry has no version because it is path/git-only or otherwise implicit.
    Unspecified,
}

impl Display for ManifestVersionSpec {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Requirement(requirement) => formatter.write_str(requirement),
            Self::WorkspaceInherited(None) => formatter.write_str("workspace = true"),
            Self::WorkspaceInherited(Some(requirement)) => {
                write!(formatter, "workspace = true ({requirement})")
            }
            Self::Unspecified => formatter.write_str(""),
        }
    }
}
