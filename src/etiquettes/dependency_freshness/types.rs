use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

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
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(self.as_str())
    }
}

/// Stable rule identifiers reserved for dependency freshness findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DependencyFreshnessRuleId {
    /// A newer compatible patch version is available.
    Patch,
    /// A newer compatible minor version is available.
    Minor,
    /// A newer major version is available.
    Major,
}

impl DependencyFreshnessRuleId {
    /// Stable string form of this value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Patch => "DEPENDENCY-FRESHNESS-PATCH",
            Self::Minor => "DEPENDENCY-FRESHNESS-MINOR",
            Self::Major => "DEPENDENCY-FRESHNESS-MAJOR",
        }
    }

    /// Parse from the stable identifier string.
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "DEPENDENCY-FRESHNESS-PATCH" => Some(Self::Patch),
            "DEPENDENCY-FRESHNESS-MINOR" => Some(Self::Minor),
            "DEPENDENCY-FRESHNESS-MAJOR" => Some(Self::Major),
            _ => None,
        }
    }
}

impl Display for DependencyFreshnessRuleId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct DependencyFreshnessRule {
    rule_id: DependencyFreshnessRuleId,
}

impl Rule for DependencyFreshnessRule {
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    fn category(&self) -> &str {
        "dependency_freshness"
    }

    fn description(&self) -> &str {
        "A newer dependency version is available according to Cargo's registry freshness view"
    }
}

#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct DependencyFreshnessMarker {
    anchor: crate::objects::NodeAnchor,
}

impl Marker for DependencyFreshnessMarker {
    fn probe(&self) -> &str {
        "dependency-freshness-site"
    }

    fn label(&self) -> &str {
        "dependency-freshness-site"
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    fn span(&self) -> Option<&dyn SourceSpan> {
        None
    }
}

#[derive(Debug, Clone, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct DependencyFreshnessFinding {
    rule: DependencyFreshnessRule,
    #[getter(copy)]
    disposition: Disposition,
    anchor: crate::objects::NodeAnchor,
    crate_name: String,
    dependency_name: String,
    package_name: String,
    span: FileSpan,
    section: String,
    version_spec: String,
    locked_versions: String,
    available_versions: String,
    update_kinds: String,
    snippet: String,
}

impl DependencyFreshnessFinding {
    pub fn builder() -> DependencyFreshnessFindingBuilder {
        DependencyFreshnessFindingBuilder::default()
    }
}

impl Finding for DependencyFreshnessFinding {
    fn rule(&self) -> &dyn Rule {
        &self.rule
    }

    fn disposition(&self) -> Disposition {
        self.disposition
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    fn emit(&self, sink: &mut dyn FindingSink) {
        sink.field("crate", &self.crate_name);
        sink.field("rule_id", &self.rule.rule_id.as_str());
        sink.field("dependency", &self.dependency_name);
        sink.field("package", &self.package_name);
        sink.field("file", &self.span.file().display().to_string());
        sink.field("line", &self.span.line().to_string());
        sink.field("section", &self.section);
        sink.field("version_spec", &self.version_spec);
        sink.field("locked_versions", &self.locked_versions);
        sink.field("available_versions", &self.available_versions);
        sink.field("update_kinds", &self.update_kinds);
        sink.field("snippet", &self.snippet);
        sink.snippet(&self.snippet);
    }
}

/// Classify a newer available version relative to a currently locked version.
///
/// Invalid semver inputs and non-newer versions return `None`; callers can
/// decide whether to record those as registry-data errors.
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

impl DependencyFreshnessIndicator {
    /// Stable string used in CSV/IR attributes.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ManifestExactPin => "manifest_exact_pin",
            Self::ManifestUpperBound => "manifest_upper_bound",
            Self::ManifestWildcard => "manifest_wildcard",
            Self::ManifestTilde => "manifest_tilde",
            Self::ManifestWorkspaceInherited => "manifest_workspace_inherited",
            Self::LockfileResolved => "lockfile_resolved",
            Self::LockfileMissing => "lockfile_missing",
            Self::PatchAvailable => "patch_available",
            Self::MinorAvailable => "minor_available",
            Self::MajorAvailable => "major_available",
        }
    }
}

impl Display for DependencyFreshnessIndicator {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(self.as_str())
    }
}

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
    pub(crate) fn from_input(input: DependencySurveyRecordInput) -> Self {
        let mut indicators = indicators_for(&input.version_spec, &input.locked_versions);
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
    pub fn locked_versions_display(&self) -> String {
        self.locked_versions.join("|")
    }

    /// Indicators formatted for a flat artifact row.
    pub fn indicators_display(&self) -> String {
        self.indicators
            .iter()
            .map(|indicator| indicator.as_str())
            .collect::<Vec<_>>()
            .join("|")
    }

    /// Available versions formatted for a flat artifact row.
    pub fn available_versions_display(&self) -> String {
        self.freshness_observations
            .iter()
            .map(|observation| observation.available_version())
            .cloned()
            .collect::<Vec<_>>()
            .join("|")
    }

    /// Registry update kinds formatted for a flat artifact row.
    pub fn update_kinds_display(&self) -> String {
        self.freshness_observations
            .iter()
            .map(|observation| observation.update_kind().as_str())
            .collect::<Vec<_>>()
            .join("|")
    }

    /// Finding rule ids formatted for a flat artifact row.
    pub fn freshness_rule_ids_display(&self) -> String {
        self.freshness_observations
            .iter()
            .map(|observation| observation.rule_id().as_str())
            .collect::<Vec<_>>()
            .join("|")
    }
}

/// Input object for constructing a dependency survey record.
#[derive(Debug, Clone)]
pub(crate) struct DependencySurveyRecordInput {
    /// Crate whose manifest contains the declaration.
    pub(crate) crate_name: String,
    /// Dependency key in the manifest.
    pub(crate) dependency_name: String,
    /// Actual package name, accounting for `package = "..."` renames.
    pub(crate) package_name: String,
    /// Manifest path that declared the dependency.
    pub(crate) manifest_path: PathBuf,
    /// Manifest line for the declaration when recoverable.
    pub(crate) line: u32,
    /// Cargo dependency section.
    pub(crate) section: DependencySection,
    /// Manifest version policy.
    pub(crate) version_spec: ManifestVersionSpec,
    /// Source class visible in the manifest.
    pub(crate) source_kind: DependencySourceKind,
    /// Versions currently resolved in `Cargo.lock`.
    pub(crate) locked_versions: Vec<String>,
}

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
