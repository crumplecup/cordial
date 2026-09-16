use super::indicator::DependencyFreshnessIndicator;
use super::rule::DependencyFreshnessRuleId;
use super::update::{DependencyUpdateKind, classify_dependency_update};

use tracing::instrument;

/// Freshness fact for one locked dependency version.
///
/// This is intentionally source-agnostic: Cargo output or a deterministic
/// cache override can both produce the same observation before an assessor
/// decides whether it should become a finding.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, derive_getters::Getters)]
pub struct DependencyFreshnessObservation {
    /// Resolved package name.
    package_name: String,
    /// Version currently present in `Cargo.lock`.
    locked_version: String,
    /// Newer version reported by the registry survey.
    available_version: String,
    /// Patch/minor/major drift class.
    #[getter(copy)]
    update_kind: DependencyUpdateKind,
    /// Survey indicator attached to the dependency row.
    #[getter(copy)]
    indicator: DependencyFreshnessIndicator,
    /// Finding rule controlled by policy for this drift class.
    #[getter(copy)]
    rule_id: DependencyFreshnessRuleId,
}

impl DependencyFreshnessObservation {
    /// Build an observation when `available_version` is newer than `locked_version`.
    ///
    /// Invalid semver inputs and non-newer versions return `None`.
    #[instrument(level = "debug", skip(package_name, locked_version, available_version))]
    pub fn from_versions(
        package_name: impl Into<String>,
        locked_version: impl Into<String>,
        available_version: impl Into<String>,
    ) -> Option<Self> {
        let locked_version = locked_version.into();
        let available_version = available_version.into();
        let update_kind = classify_dependency_update(&locked_version, &available_version)?;
        Some(Self {
            package_name: package_name.into(),
            locked_version,
            available_version,
            update_kind,
            indicator: update_kind.indicator(),
            rule_id: update_kind.rule_id(),
        })
    }
}
