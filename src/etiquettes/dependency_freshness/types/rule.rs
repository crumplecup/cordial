use std::fmt::{Display, Formatter, Result as FmtResult};

use serde::{Deserialize, Serialize};

use crate::objects::Rule;

use tracing::instrument;

/// Stable rule identifiers reserved for dependency freshness findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DependencyFreshnessRuleId {
    /// A newer compatible patch version is available.
    Patch,
    /// A newer compatible minor version is available.
    Minor,
    /// A newer major version is available.
    Major,
    /// A manifest requirement uses an exact `=` pin.
    ManifestExactPin,
    /// A manifest requirement uses an upper bound.
    ManifestUpperBound,
    /// A manifest requirement uses a wildcard.
    ManifestWildcard,
    /// A manifest requirement uses a tilde constraint.
    ManifestTilde,
    /// A member dependency bypasses a matching workspace dependency policy.
    ManifestWorkspaceBypass,
}

impl DependencyFreshnessRuleId {
    /// Stable string form of this value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Patch => "DEPENDENCY-FRESHNESS-PATCH",
            Self::Minor => "DEPENDENCY-FRESHNESS-MINOR",
            Self::Major => "DEPENDENCY-FRESHNESS-MAJOR",
            Self::ManifestExactPin => "DEPENDENCY-FRESHNESS-MANIFEST-EXACT-PIN",
            Self::ManifestUpperBound => "DEPENDENCY-FRESHNESS-MANIFEST-UPPER-BOUND",
            Self::ManifestWildcard => "DEPENDENCY-FRESHNESS-MANIFEST-WILDCARD",
            Self::ManifestTilde => "DEPENDENCY-FRESHNESS-MANIFEST-TILDE",
            Self::ManifestWorkspaceBypass => "DEPENDENCY-FRESHNESS-MANIFEST-WORKSPACE-BYPASS",
        }
    }

    /// Parse from the stable identifier string.
    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "DEPENDENCY-FRESHNESS-PATCH" => Some(Self::Patch),
            "DEPENDENCY-FRESHNESS-MINOR" => Some(Self::Minor),
            "DEPENDENCY-FRESHNESS-MAJOR" => Some(Self::Major),
            "DEPENDENCY-FRESHNESS-MANIFEST-EXACT-PIN" => Some(Self::ManifestExactPin),
            "DEPENDENCY-FRESHNESS-MANIFEST-UPPER-BOUND" => Some(Self::ManifestUpperBound),
            "DEPENDENCY-FRESHNESS-MANIFEST-WILDCARD" => Some(Self::ManifestWildcard),
            "DEPENDENCY-FRESHNESS-MANIFEST-TILDE" => Some(Self::ManifestTilde),
            "DEPENDENCY-FRESHNESS-MANIFEST-WORKSPACE-BYPASS" => Some(Self::ManifestWorkspaceBypass),
            _ => None,
        }
    }
}

impl Display for DependencyFreshnessRuleId {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, derive_getters::Getters, derive_new::new)]
pub struct DependencyFreshnessRule {
    #[getter(copy)]
    rule_id: DependencyFreshnessRuleId,
}

impl Rule for DependencyFreshnessRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "dependency_freshness"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        match self.rule_id {
            DependencyFreshnessRuleId::Patch => {
                "A newer patch dependency version is available according to Cargo's registry freshness view"
            }
            DependencyFreshnessRuleId::Minor => {
                "A newer minor dependency version is available according to Cargo's registry freshness view"
            }
            DependencyFreshnessRuleId::Major => {
                "A newer major dependency version is available according to Cargo's registry freshness view"
            }
            DependencyFreshnessRuleId::ManifestExactPin => {
                "A dependency manifest uses an exact version pin"
            }
            DependencyFreshnessRuleId::ManifestUpperBound => {
                "A dependency manifest uses an upper-bound version requirement"
            }
            DependencyFreshnessRuleId::ManifestWildcard => {
                "A dependency manifest uses a wildcard version requirement"
            }
            DependencyFreshnessRuleId::ManifestTilde => {
                "A dependency manifest uses a tilde version requirement"
            }
            DependencyFreshnessRuleId::ManifestWorkspaceBypass => {
                "A member dependency bypasses a matching workspace dependency policy"
            }
        }
    }
}
