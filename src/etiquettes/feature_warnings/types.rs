//! Feature-dependent compiler warnings.

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;

/// Stable rule identifier for a feature-dependent warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeatureWarningRuleId {
    /// An `unused_*` (or other non-dead-code) warning under some combinations.
    Unused001,
    /// A `dead_code` warning under some combinations.
    DeadCode002,
    /// A feature combination that does not compile.
    Failure003,
}

impl FeatureWarningRuleId {
    /// Stable string form of this value.
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unused001 => "FEATURE-WARNING-001",
            Self::DeadCode002 => "FEATURE-WARNING-002",
            Self::Failure003 => "FEATURE-WARNING-003",
        }
    }

    /// Parse from the stable identifier string.
    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "FEATURE-WARNING-001" => Some(Self::Unused001),
            "FEATURE-WARNING-002" => Some(Self::DeadCode002),
            "FEATURE-WARNING-003" => Some(Self::Failure003),
            _ => None,
        }
    }

    /// The rule a rustc lint code reports under.
    #[instrument(level = "debug")]
    pub fn for_lint(lint: &str) -> Self {
        if lint == "dead_code" {
            Self::DeadCode002
        } else {
            Self::Unused001
        }
    }
}

impl Display for FeatureWarningRuleId {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct FeatureWarningRule {
    rule_id: FeatureWarningRuleId,
}

impl Rule for FeatureWarningRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "feature_warnings"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        match self.rule_id {
            FeatureWarningRuleId::Unused001 => {
                "unused import or binding under some feature combinations"
            }
            FeatureWarningRuleId::DeadCode002 => "dead code under some feature combinations",
            FeatureWarningRuleId::Failure003 => "feature combination does not compile",
        }
    }
}

#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct FeatureWarningMarker {
    anchor: crate::objects::NodeAnchor,
}

impl Marker for FeatureWarningMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "feature-warning-site"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "feature-warning-site"
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
pub struct FeatureWarningFinding {
    rule: FeatureWarningRule,
    #[getter(copy)]
    disposition: Disposition,
    anchor: crate::objects::NodeAnchor,
    crate_name: String,
    lint: String,
    span: FileSpan,
    message: String,
    gate: String,
    #[getter(copy)]
    wide: bool,
    advice: String,
    triggering: String,
}

impl FeatureWarningFinding {
    #[instrument(level = "debug")]
    pub fn builder() -> FeatureWarningFindingBuilder {
        FeatureWarningFindingBuilder::default()
    }
}

impl Finding for FeatureWarningFinding {
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
        sink.field("lint", &self.lint);
        sink.field("file", &self.span.file().display().to_string());
        sink.field("line", &self.span.line().to_string());
        sink.field("message", &self.message);
        sink.field("gate", &self.gate);
        sink.field("wide", if self.wide { &"true" } else { &"false" });
        sink.field("advice", &self.advice);
        sink.field("triggering", &self.triggering);
        sink.snippet(&self.message);
    }
}

/// One feature-dependent warning site, ready to become an IR node.
#[derive(Debug, Clone, PartialEq, Eq, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct FeatureWarningRecord {
    /// Stable probe rule identifier.
    #[getter(copy)]
    rule_id: FeatureWarningRuleId,
    /// rustc lint code (`unused_imports`, `dead_code`, ...).
    lint: String,
    /// Source file path as cargo reported it.
    file: PathBuf,
    /// Source line number (1-based).
    #[getter(copy)]
    line: u32,
    /// The warning message (names merged across combinations).
    message: String,
    /// Suggested `cfg` predicate, or empty when no single gate fits.
    gate: String,
    /// Whether the gate names more features than the private-feature
    /// threshold, so a private feature is advised instead.
    #[getter(copy)]
    wide: bool,
    /// What to do about it.
    advice: String,
    /// "N of M combinations, e.g. `--features a,b`".
    triggering: String,
}

impl FeatureWarningRecord {
    /// Start a builder for this value.
    #[instrument(level = "debug")]
    pub fn builder() -> FeatureWarningRecordBuilder {
        FeatureWarningRecordBuilder::default()
    }
}
