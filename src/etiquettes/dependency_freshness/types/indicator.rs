use std::fmt::{Display, Formatter, Result as FmtResult};

use super::rule::DependencyFreshnessRuleId;

use tracing::instrument;

/// Survey-level indicator; later lints can promote selected indicators into findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DependencyFreshnessIndicator {
    /// Manifest pins an exact version using `=`.
    ManifestExactPin,
    /// Manifest contains an upper bound using `<`.
    ManifestUpperBound,
    /// Manifest uses a wildcard requirement.
    ManifestWildcard,
    /// Manifest uses a tilde requirement.
    ManifestTilde,
    /// Member manifest declares local policy where `[workspace.dependencies]` exists.
    ManifestWorkspaceBypass,
    /// Member manifest inherits dependency policy from the workspace.
    ManifestWorkspaceInherited,
    /// Lockfile contains at least one resolved version for this package.
    LockfileResolved,
    /// Lockfile does not contain a resolved version for this package.
    LockfileMissing,
    /// Future registry survey: a newer patch release is available.
    PatchAvailable,
    /// Future registry survey: a newer minor release is available.
    MinorAvailable,
    /// Future registry survey: a newer major release is available.
    MajorAvailable,
}

impl DependencyFreshnessIndicator {
    /// Stable string used in CSV/IR attributes.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ManifestExactPin => "manifest_exact_pin",
            Self::ManifestUpperBound => "manifest_upper_bound",
            Self::ManifestWildcard => "manifest_wildcard",
            Self::ManifestTilde => "manifest_tilde",
            Self::ManifestWorkspaceBypass => "manifest_workspace_bypass",
            Self::ManifestWorkspaceInherited => "manifest_workspace_inherited",
            Self::LockfileResolved => "lockfile_resolved",
            Self::LockfileMissing => "lockfile_missing",
            Self::PatchAvailable => "patch_available",
            Self::MinorAvailable => "minor_available",
            Self::MajorAvailable => "major_available",
        }
    }

    /// Finding rule promoted from manifest-policy indicators.
    pub const fn manifest_rule_id(self) -> Option<DependencyFreshnessRuleId> {
        match self {
            Self::ManifestExactPin => Some(DependencyFreshnessRuleId::ManifestExactPin),
            Self::ManifestUpperBound => Some(DependencyFreshnessRuleId::ManifestUpperBound),
            Self::ManifestWildcard => Some(DependencyFreshnessRuleId::ManifestWildcard),
            Self::ManifestTilde => Some(DependencyFreshnessRuleId::ManifestTilde),
            Self::ManifestWorkspaceBypass => {
                Some(DependencyFreshnessRuleId::ManifestWorkspaceBypass)
            }
            Self::ManifestWorkspaceInherited
            | Self::LockfileResolved
            | Self::LockfileMissing
            | Self::PatchAvailable
            | Self::MinorAvailable
            | Self::MajorAvailable => None,
        }
    }
}

impl Display for DependencyFreshnessIndicator {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(self.as_str())
    }
}
