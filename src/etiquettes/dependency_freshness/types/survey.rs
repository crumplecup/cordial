use std::path::PathBuf;

use super::indicator::DependencyFreshnessIndicator;
use super::manifest::{DependencySection, DependencySourceKind, ManifestVersionSpec};
use super::observation::DependencyFreshnessObservation;

use tracing::instrument;

/// One direct dependency declaration joined with any lockfile resolution.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct DependencySurveyRecord {
    /// Crate whose manifest contains the declaration.
    crate_name: String,
    /// Dependency key in the manifest.
    dependency_name: String,
    /// Actual package name, accounting for `package = "..."` renames.
    package_name: String,
    /// Manifest path that declared the dependency.
    manifest_path: PathBuf,
    /// Manifest line for the declaration when recoverable.
    line: u32,
    /// Cargo dependency section.
    section: DependencySection,
    /// Manifest version policy.
    version_spec: ManifestVersionSpec,
    /// Source class visible in the manifest.
    source_kind: DependencySourceKind,
    /// Versions currently resolved in `Cargo.lock`.
    locked_versions: Vec<String>,
    /// Registry freshness observations matched to locked versions.
    freshness_observations: Vec<DependencyFreshnessObservation>,
    /// Survey indicators available before registry freshness is queried.
    indicators: Vec<DependencyFreshnessIndicator>,
}

impl DependencySurveyRecord {
    /// Construct a dependency survey record.
    #[instrument(level = "debug", skip(input))]
    pub(crate) fn from_input(input: DependencySurveyRecordInput) -> Self {
        let mut indicators = indicators_for(&input.version_spec, &input.locked_versions);
        indicators.extend(input.indicators);
        indicators.sort();
        indicators.dedup();
        Self {
            crate_name: input.crate_name,
            dependency_name: input.dependency_name,
            package_name: input.package_name,
            manifest_path: input.manifest_path,
            line: input.line,
            section: input.section,
            version_spec: input.version_spec,
            source_kind: input.source_kind,
            locked_versions: input.locked_versions,
            freshness_observations: Vec::new(),
            indicators,
        }
    }

    /// Attach one registry freshness observation to this dependency row.
    #[instrument(level = "debug", skip(self, observation))]
    pub(crate) fn add_freshness_observation(
        &mut self,
        observation: DependencyFreshnessObservation,
    ) {
        self.indicators.push(observation.indicator());
        self.indicators.sort();
        self.indicators.dedup();
        self.freshness_observations.push(observation);
        self.freshness_observations.sort();
        self.freshness_observations.dedup();
    }

    /// `Cargo.lock` versions formatted for a flat artifact row.
    #[instrument(level = "trace", skip(self))]
    pub fn locked_versions_display(&self) -> String {
        self.locked_versions.join("|")
    }

    /// Indicators formatted for a flat artifact row.
    #[instrument(level = "trace", skip(self))]
    pub fn indicators_display(&self) -> String {
        self.indicators
            .iter()
            .map(|indicator| indicator.as_str())
            .collect::<Vec<_>>()
            .join("|")
    }

    /// Available versions formatted for a flat artifact row.
    #[instrument(level = "trace", skip(self))]
    pub fn available_versions_display(&self) -> String {
        self.freshness_observations
            .iter()
            .map(|observation| observation.available_version())
            .cloned()
            .collect::<Vec<_>>()
            .join("|")
    }

    /// Registry update kinds formatted for a flat artifact row.
    #[instrument(level = "trace", skip(self))]
    pub fn update_kinds_display(&self) -> String {
        self.freshness_observations
            .iter()
            .map(|observation| observation.update_kind().as_str())
            .collect::<Vec<_>>()
            .join("|")
    }

    /// Finding rule ids formatted for a flat artifact row.
    #[instrument(level = "debug", skip(self))]
    pub fn freshness_rule_ids_display(&self) -> String {
        let mut rule_ids = self
            .freshness_observations
            .iter()
            .map(|observation| observation.rule_id())
            .chain(
                self.indicators
                    .iter()
                    .filter_map(|indicator| indicator.manifest_rule_id()),
            )
            .collect::<Vec<_>>();
        rule_ids.sort();
        rule_ids.dedup();
        rule_ids
            .iter()
            .map(|rule_id| rule_id.as_str())
            .collect::<Vec<_>>()
            .join("|")
    }
}

/// Input object for constructing a dependency survey record.
#[derive(Debug, Clone, derive_builder::Builder)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub(crate) struct DependencySurveyRecordInput {
    /// Crate whose manifest contains the declaration.
    crate_name: String,
    /// Dependency key in the manifest.
    dependency_name: String,
    /// Actual package name, accounting for `package = "..."` renames.
    package_name: String,
    /// Manifest path that declared the dependency.
    manifest_path: PathBuf,
    /// Manifest line for the declaration when recoverable.
    line: u32,
    /// Cargo dependency section.
    section: DependencySection,
    /// Manifest version policy.
    version_spec: ManifestVersionSpec,
    /// Source class visible in the manifest.
    source_kind: DependencySourceKind,
    /// Versions currently resolved in `Cargo.lock`.
    locked_versions: Vec<String>,
    /// Additional manifest-structure indicators collected by the scanner.
    indicators: Vec<DependencyFreshnessIndicator>,
}

impl DependencySurveyRecordInput {
    /// Start building a dependency survey record input.
    #[instrument(level = "debug")]
    pub(crate) fn builder() -> DependencySurveyRecordInputBuilder {
        DependencySurveyRecordInputBuilder::default()
    }
}

#[instrument(level = "debug", skip(version_spec))]
fn indicators_for(
    version_spec: &ManifestVersionSpec,
    locked_versions: &[String],
) -> Vec<DependencyFreshnessIndicator> {
    let mut indicators = Vec::new();
    match version_spec {
        ManifestVersionSpec::Requirement(requirement) => {
            requirement_indicators(requirement, &mut indicators);
        }
        ManifestVersionSpec::WorkspaceInherited(requirement) => {
            indicators.push(DependencyFreshnessIndicator::ManifestWorkspaceInherited);
            if let Some(requirement) = requirement {
                requirement_indicators(requirement, &mut indicators);
            }
        }
        ManifestVersionSpec::Unspecified => {}
    }
    if locked_versions.is_empty() {
        indicators.push(DependencyFreshnessIndicator::LockfileMissing);
    } else {
        indicators.push(DependencyFreshnessIndicator::LockfileResolved);
    }
    indicators
}

#[instrument(level = "debug", skip(indicators))]
fn requirement_indicators(requirement: &str, indicators: &mut Vec<DependencyFreshnessIndicator>) {
    if requirement.trim_start().starts_with('=') {
        indicators.push(DependencyFreshnessIndicator::ManifestExactPin);
    }
    if requirement.contains('<') {
        indicators.push(DependencyFreshnessIndicator::ManifestUpperBound);
    }
    if requirement.contains('*') {
        indicators.push(DependencyFreshnessIndicator::ManifestWildcard);
    }
    if requirement.trim_start().starts_with('~') {
        indicators.push(DependencyFreshnessIndicator::ManifestTilde);
    }
}
