use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;

/// Stable rule identifier for a test living under `src/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InlineTestRuleId {
    /// `#[cfg(test)] mod …`
    Mod001,
    /// `#[cfg(test)]` on a non-mod item.
    Cfg001,
    /// `#[test]` (or `tokio::test` / `rstest`) outside a flagged test module.
    Fn001,
}

impl InlineTestRuleId {
    /// Stable string form of this value.
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mod001 => "INLINE-TEST-MOD",
            Self::Cfg001 => "INLINE-TEST-CFG",
            Self::Fn001 => "INLINE-TEST-FN",
        }
    }

    /// Parse from the stable identifier string.
    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "INLINE-TEST-MOD" => Some(Self::Mod001),
            "INLINE-TEST-CFG" => Some(Self::Cfg001),
            "INLINE-TEST-FN" => Some(Self::Fn001),
            _ => None,
        }
    }
}

impl Display for InlineTestRuleId {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct InlineTestRule {
    rule_id: InlineTestRuleId,
}

impl Rule for InlineTestRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "inline_tests"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Test code under `src/` — move it to the crate `tests/` directory"
    }
}

#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct InlineTestMarker {
    anchor: crate::objects::NodeAnchor,
}

impl Marker for InlineTestMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "inline-test-site"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "inline-test-site"
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
pub struct InlineTestFinding {
    rule: InlineTestRule,
    #[getter(copy)]
    disposition: Disposition,
    anchor: crate::objects::NodeAnchor,
    crate_name: String,
    context: String,
    span: FileSpan,
    snippet: String,
}

impl InlineTestFinding {
    pub fn builder() -> InlineTestFindingBuilder {
        InlineTestFindingBuilder::default()
    }
}

impl Finding for InlineTestFinding {
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
#[derive(Debug, Clone, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct InlineTestSiteRecord {
    #[getter(copy)]
    rule_id: InlineTestRuleId,
    context: String,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    snippet: String,
}

impl InlineTestSiteRecord {
    pub fn builder() -> InlineTestSiteRecordBuilder {
        InlineTestSiteRecordBuilder::default()
    }
}
