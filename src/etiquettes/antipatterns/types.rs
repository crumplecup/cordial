use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::CordialResult;
use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;
/// Stable rule identifier for an antipattern probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AntipatternRuleId {
    /// `ANTIPATTERN-BOX-DYN-ERROR-001`.
    BoxDynError001,
    /// `ANTIPATTERN-STRING-ERROR-001`.
    StringError001,
    /// `ANTIPATTERN-UNUSED-UNDERSCORE-ARG-001`.
    UnusedUnderscoreArg001,
    /// `ANTIPATTERN-STRUCT-STATIC-REF-001`.
    StructStaticRef001,
    /// `ANTIPATTERN-UNNAMED-CONTRACT-BOUND-001`.
    UnnamedContractBound001,
    /// `ANTIPATTERN-VERSION-IN-MEMBER-001`.
    VersionInMember001,
}

impl AntipatternRuleId {
    /// Stable string form of this value.
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BoxDynError001 => "ANTIPATTERN-BOX-DYN-ERROR-001",
            Self::StringError001 => "ANTIPATTERN-STRING-ERROR-001",
            Self::UnusedUnderscoreArg001 => "ANTIPATTERN-UNUSED-UNDERSCORE-ARG-001",
            Self::StructStaticRef001 => "ANTIPATTERN-STRUCT-STATIC-REF-001",
            Self::UnnamedContractBound001 => "ANTIPATTERN-UNNAMED-CONTRACT-BOUND-001",
            Self::VersionInMember001 => "ANTIPATTERN-VERSION-IN-MEMBER-001",
        }
    }

    /// Parse from the stable identifier string.
    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "ANTIPATTERN-BOX-DYN-ERROR-001" => Some(Self::BoxDynError001),
            "ANTIPATTERN-STRING-ERROR-001" => Some(Self::StringError001),
            "ANTIPATTERN-UNUSED-UNDERSCORE-ARG-001" => Some(Self::UnusedUnderscoreArg001),
            "ANTIPATTERN-STRUCT-STATIC-REF-001" => Some(Self::StructStaticRef001),
            "ANTIPATTERN-UNNAMED-CONTRACT-BOUND-001" => Some(Self::UnnamedContractBound001),
            "ANTIPATTERN-VERSION-IN-MEMBER-001" => Some(Self::VersionInMember001),
            _ => None,
        }
    }
}

impl Display for AntipatternRuleId {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct AntipatternRule {
    rule_id: AntipatternRuleId,
}

impl Rule for AntipatternRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "antipatterns"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        match self.rule_id {
            AntipatternRuleId::BoxDynError001 => {
                "`Box<dyn Error>` used as an error carrier instead of a typed error"
            }
            AntipatternRuleId::StringError001 => {
                "`String` / `&str` used as a `Result` error type instead of a newtype"
            }
            AntipatternRuleId::UnusedUnderscoreArg001 => {
                "Function parameter bound with a leading underscore"
            }
            AntipatternRuleId::StructStaticRef001 => {
                "ADT field typed as `&'static T` instead of an owning type \
                 (copy `file`/`line` out of `Location`; `&'static dyn` of a crate-local trait \
                 is exempt; `&'static str` is allowed on types constructed only as `const`/`static`)"
            }
            AntipatternRuleId::UnnamedContractBound001 => {
                "Requires/ensures clause matching no registered contract fragment"
            }
            AntipatternRuleId::VersionInMember001 => {
                "Inline `version` in a workspace member manifest"
            }
        }
    }
}

#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct AntipatternMarker {
    anchor: crate::objects::NodeAnchor,
}

impl Marker for AntipatternMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "antipattern-site"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "antipattern-site"
    }

    #[instrument(level = "trace", skip(self))]
    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    #[instrument(level = "trace", skip(self))]
    fn span(&self) -> Option<&dyn SourceSpan> {
        None
    }
}

#[derive(Debug, Clone, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct AntipatternFinding {
    rule: AntipatternRule,
    #[getter(copy)]
    disposition: Disposition,
    anchor: crate::objects::NodeAnchor,
    crate_name: String,
    context: String,
    span: FileSpan,
    snippet: String,
}

impl AntipatternFinding {
    /// Start a builder for this value.
    pub fn builder() -> AntipatternFindingBuilder {
        AntipatternFindingBuilder::default()
    }
}

impl Finding for AntipatternFinding {
    #[instrument(level = "trace", skip(self))]
    fn rule(&self) -> &dyn Rule {
        &self.rule
    }

    #[instrument(level = "trace", skip(self))]
    fn disposition(&self) -> Disposition {
        self.disposition
    }

    #[instrument(level = "trace", skip(self))]
    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    #[instrument(level = "trace", skip(self, sink))]
    fn emit(&self, sink: &mut dyn FindingSink) {
        sink.field("crate", &self.crate_name);
        sink.field("rule_id", &self.rule.rule_id);
        sink.field("context", &self.context);
        sink.field("file", &self.span.file().display().to_string());
        sink.field("line", &self.span.line().to_string());
        sink.field("snippet", &self.snippet);
        sink.snippet(&self.snippet);
    }
}

/// Raw scan row used while building IR nodes.
#[derive(Debug, Clone, PartialEq, Eq, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct AntipatternSiteRecord {
    /// Stable probe rule identifier.
    #[getter(copy)]
    rule_id: AntipatternRuleId,
    /// Qualified name or extra locator for this site.
    context: String,
    /// Source file path, usually crate-relative.
    file: PathBuf,
    /// Source line number (1-based), when known.
    #[getter(copy)]
    line: u32,
    /// Source snippet captured at the site.
    snippet: String,
}

impl AntipatternSiteRecord {
    /// Start a builder for this value.
    pub fn builder() -> AntipatternSiteRecordBuilder {
        AntipatternSiteRecordBuilder::default()
    }
}

/// Count findings by rule for summaries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AntipatternRuleCounts {
    pub box_dyn_error: usize,
    pub string_error: usize,
    pub unused_underscore_arg: usize,
    pub struct_static_ref: usize,
    pub unnamed_contract_bound: usize,
    pub version_in_member: usize,
}

impl AntipatternRuleCounts {
    #[instrument(level = "debug", skip(self, rule_id))]
    pub fn accumulate(&mut self, rule_id: AntipatternRuleId) {
        match rule_id {
            AntipatternRuleId::BoxDynError001 => self.box_dyn_error += 1,
            AntipatternRuleId::StringError001 => self.string_error += 1,
            AntipatternRuleId::UnusedUnderscoreArg001 => self.unused_underscore_arg += 1,
            AntipatternRuleId::StructStaticRef001 => self.struct_static_ref += 1,
            AntipatternRuleId::UnnamedContractBound001 => self.unnamed_contract_bound += 1,
            AntipatternRuleId::VersionInMember001 => self.version_in_member += 1,
        }
    }
}

/// Per-crate rollup row.
#[derive(Debug, Clone, PartialEq, Eq, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct AntipatternCrateSummary {
    crate_name: String,
    #[getter(copy)]
    total: usize,
    #[getter(copy)]
    box_dyn_error: usize,
    #[getter(copy)]
    string_error: usize,
    #[getter(copy)]
    unused_underscore_arg: usize,
    #[getter(copy)]
    struct_static_ref: usize,
    #[getter(copy)]
    unnamed_contract_bound: usize,
    #[getter(copy)]
    version_in_member: usize,
}

impl AntipatternCrateSummary {
    /// Start a builder for this value.
    pub fn builder() -> AntipatternCrateSummaryBuilder {
        AntipatternCrateSummaryBuilder::default()
    }
}

/// Workspace rollup across crates.
#[derive(Debug, Clone, PartialEq, Eq, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct WorkspaceAntipatternsSummary {
    #[getter(copy)]
    total: usize,
    #[getter(copy)]
    box_dyn_error: usize,
    #[getter(copy)]
    string_error: usize,
    #[getter(copy)]
    unused_underscore_arg: usize,
    #[getter(copy)]
    struct_static_ref: usize,
    #[getter(copy)]
    unnamed_contract_bound: usize,
    #[getter(copy)]
    version_in_member: usize,
    crates: Vec<AntipatternCrateSummary>,
}

impl WorkspaceAntipatternsSummary {
    /// Start a builder for this value.
    pub fn builder() -> WorkspaceAntipatternsSummaryBuilder {
        WorkspaceAntipatternsSummaryBuilder::default()
    }
}

/// Per-crate rollup row for version-in-member scans.
#[derive(Debug, Clone, PartialEq, Eq, derive_new::new, derive_getters::Getters)]
pub struct VersionInMemberCrateSummary {
    crate_name: String,
    #[getter(copy)]
    total: usize,
}

/// Workspace rollup for version-in-member scans.
#[derive(Debug, Clone, PartialEq, Eq, derive_new::new, derive_getters::Getters)]
pub struct WorkspaceVersionInMemberSummary {
    #[getter(copy)]
    total: usize,
    #[getter(copy)]
    crates_with_findings: usize,
    crates: Vec<VersionInMemberCrateSummary>,
}

#[instrument(level = "debug", skip(findings))]
pub fn build_workspace_antipatterns_summary(
    findings: &[&dyn Finding],
) -> CordialResult<WorkspaceAntipatternsSummary> {
    let mut by_crate: std::collections::BTreeMap<String, AntipatternRuleCounts> =
        std::collections::BTreeMap::new();

    for finding in findings {
        if finding.rule().category() != "antipatterns" || finding.disposition() != Disposition::Open
        {
            continue;
        }
        let mut sink = crate::objects::MapFindingSink::default();
        finding.emit(&mut sink);
        let crate_name = sink
            .fields
            .iter()
            .find(|(key, _)| key == "crate")
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let rule_id = sink
            .fields
            .iter()
            .find(|(key, _)| key == "rule_id")
            .and_then(|(_, value)| AntipatternRuleId::from_attr(value));
        let Some(rule_id) = rule_id else {
            continue;
        };
        by_crate.entry(crate_name).or_default().accumulate(rule_id);
    }

    let mut crates = Vec::new();
    let mut totals = AntipatternRuleCounts::default();
    for (crate_name, counts) in by_crate {
        let total = counts.box_dyn_error
            + counts.string_error
            + counts.unused_underscore_arg
            + counts.struct_static_ref
            + counts.unnamed_contract_bound
            + counts.version_in_member;
        totals.box_dyn_error += counts.box_dyn_error;
        totals.string_error += counts.string_error;
        totals.unused_underscore_arg += counts.unused_underscore_arg;
        totals.struct_static_ref += counts.struct_static_ref;
        totals.unnamed_contract_bound += counts.unnamed_contract_bound;
        totals.version_in_member += counts.version_in_member;
        crates.push(
            AntipatternCrateSummary::builder()
                .crate_name(crate_name)
                .total(total)
                .box_dyn_error(counts.box_dyn_error)
                .string_error(counts.string_error)
                .unused_underscore_arg(counts.unused_underscore_arg)
                .struct_static_ref(counts.struct_static_ref)
                .unnamed_contract_bound(counts.unnamed_contract_bound)
                .version_in_member(counts.version_in_member)
                .build()?,
        );
    }

    let total = totals.box_dyn_error
        + totals.string_error
        + totals.unused_underscore_arg
        + totals.struct_static_ref
        + totals.unnamed_contract_bound
        + totals.version_in_member;

    WorkspaceAntipatternsSummary::builder()
        .total(total)
        .box_dyn_error(totals.box_dyn_error)
        .string_error(totals.string_error)
        .unused_underscore_arg(totals.unused_underscore_arg)
        .struct_static_ref(totals.struct_static_ref)
        .unnamed_contract_bound(totals.unnamed_contract_bound)
        .version_in_member(totals.version_in_member)
        .crates(crates)
        .build()
}

#[instrument(level = "debug", skip(findings))]
pub fn build_workspace_version_in_member_summary(
    findings: &[&dyn Finding],
) -> WorkspaceVersionInMemberSummary {
    let mut by_crate: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();

    for finding in findings {
        if finding.rule().category() != "antipatterns"
            || finding.rule().id() != AntipatternRuleId::VersionInMember001.as_str()
            || finding.disposition() != Disposition::Open
        {
            continue;
        }
        let mut sink = crate::objects::MapFindingSink::default();
        finding.emit(&mut sink);
        let crate_name = sink
            .fields
            .iter()
            .find(|(key, _)| key == "crate")
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| "unknown".to_string());
        *by_crate.entry(crate_name).or_default() += 1;
    }

    let crates: Vec<_> = by_crate
        .into_iter()
        .map(|(crate_name, total)| VersionInMemberCrateSummary::new(crate_name, total))
        .collect();
    let total = crates.iter().map(|row| row.total()).sum();

    WorkspaceVersionInMemberSummary::new(total, crates.len(), crates)
}
