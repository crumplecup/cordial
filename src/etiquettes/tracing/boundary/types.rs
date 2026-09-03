use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;

/// Stable rule identifier for a binary-error-boundary finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BoundaryRuleId {
    /// A fallible `fn main` in a binary never converts its error to a
    /// tracing warn/error emission before the process boundary.
    MainSilent,
}

impl BoundaryRuleId {
    /// Stable string form of this value.
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MainSilent => "TRACING-BOUNDARY-MAIN-SILENT",
        }
    }

    /// Parse from the stable identifier string.
    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "TRACING-BOUNDARY-MAIN-SILENT" => Some(Self::MainSilent),
            _ => None,
        }
    }

    /// Whether `id` is a binary-error-boundary rule (`TRACING-BOUNDARY-*`).
    #[instrument(level = "debug")]
    pub fn is_boundary_rule(id: &str) -> bool {
        id.starts_with("TRACING-BOUNDARY-")
    }

    #[instrument(level = "debug", skip(self))]
    fn description(self) -> &'static str {
        match self {
            Self::MainSilent => {
                "Fallible fn main never reports its error via tracing before returning — \
                 add #[instrument(err(...))] or emit tracing::warn!/error! on the error path"
            }
        }
    }
}

impl Display for BoundaryRuleId {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct BoundaryRule {
    rule_id: BoundaryRuleId,
}

impl Rule for BoundaryRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "tracing"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        self.rule_id.description()
    }
}

pub const BOUNDARY_SITE_LABEL: &str = "tracing-boundary-site";

#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct BoundaryMarker {
    anchor: crate::objects::NodeAnchor,
}

impl Marker for BoundaryMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        BOUNDARY_SITE_LABEL
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        BOUNDARY_SITE_LABEL
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
pub struct BoundaryFinding {
    rule: BoundaryRule,
    #[getter(copy)]
    disposition: Disposition,
    anchor: crate::objects::NodeAnchor,
    crate_name: String,
    context: String,
    span: FileSpan,
    snippet: String,
}

impl BoundaryFinding {
    /// Start a builder for this value.
    pub fn builder() -> BoundaryFindingBuilder {
        BoundaryFindingBuilder::default()
    }
}

impl Finding for BoundaryFinding {
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
        sink.field("rule", &self.rule.rule_id);
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
pub struct BoundarySiteRecord {
    /// Stable probe rule identifier.
    #[getter(copy)]
    rule_id: BoundaryRuleId,
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

impl BoundarySiteRecord {
    /// Start a builder for this value.
    pub fn builder() -> BoundarySiteRecordBuilder {
        BoundarySiteRecordBuilder::default()
    }
}
