use std::fmt::{Display, Formatter, Result as FmtResult};

use serde::{Deserialize, Serialize};

use super::indicator::DependencyFreshnessIndicator;
use super::rule::DependencyFreshnessRuleId;

use tracing::instrument;

/// Kind of newer version available for one locked dependency version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyUpdateKind {
    /// Same major/minor, newer patch or prerelease.
    Patch,
    /// Same major, newer minor.
    Minor,
    /// Newer major.
    Major,
}

impl DependencyUpdateKind {
    /// Stable string used in CSV/IR attributes.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Patch => "patch",
            Self::Minor => "minor",
            Self::Major => "major",
        }
    }

    /// Survey indicator produced when a registry source reports this update kind.
    pub const fn indicator(self) -> DependencyFreshnessIndicator {
        match self {
            Self::Patch => DependencyFreshnessIndicator::PatchAvailable,
            Self::Minor => DependencyFreshnessIndicator::MinorAvailable,
            Self::Major => DependencyFreshnessIndicator::MajorAvailable,
        }
    }

    /// Rule id that governs findings for this update kind.
    pub const fn rule_id(self) -> DependencyFreshnessRuleId {
        match self {
            Self::Patch => DependencyFreshnessRuleId::Patch,
            Self::Minor => DependencyFreshnessRuleId::Minor,
            Self::Major => DependencyFreshnessRuleId::Major,
        }
    }
}

impl Display for DependencyUpdateKind {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(self.as_str())
    }
}

/// Classify a newer available version relative to a currently locked version.
///
/// Invalid semver inputs and non-newer versions return `None`; callers can
/// decide whether to record those as registry-data errors.
#[instrument(level = "debug")]
pub fn classify_dependency_update(current: &str, available: &str) -> Option<DependencyUpdateKind> {
    let current = semver::Version::parse(current).ok()?;
    let available = semver::Version::parse(available).ok()?;
    if available <= current {
        return None;
    }
    if available.major != current.major {
        Some(DependencyUpdateKind::Major)
    } else if available.minor != current.minor {
        Some(DependencyUpdateKind::Minor)
    } else {
        Some(DependencyUpdateKind::Patch)
    }
}
