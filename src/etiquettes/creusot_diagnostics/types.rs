use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;

/// Stable rule identifier for a Creusot diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CreusotDiagnosticRuleId {
    /// A `warning:` diagnostic from `cargo creusot prove`.
    Warning001,
    /// An `error:` diagnostic or failed `cargo creusot prove` run.
    Failure001,
}

impl CreusotDiagnosticRuleId {
    /// Stable string form of this value.
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warning001 => "CREUSOT-DIAGNOSTIC-001",
            Self::Failure001 => "CREUSOT-DIAGNOSTIC-002",
        }
    }

    /// Parse from the stable identifier string.
    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "CREUSOT-DIAGNOSTIC-001" => Some(Self::Warning001),
            "CREUSOT-DIAGNOSTIC-002" => Some(Self::Failure001),
            _ => None,
        }
    }
}

impl Display for CreusotDiagnosticRuleId {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct CreusotDiagnosticRule {
    rule_id: CreusotDiagnosticRuleId,
}

impl Rule for CreusotDiagnosticRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "creusot_diagnostics"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Creusot prove diagnostic — rustc/clippy never see this warning or verifier failure"
    }
}

#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct CreusotDiagnosticMarker {
    anchor: crate::objects::NodeAnchor,
}

impl Marker for CreusotDiagnosticMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "creusot-diagnostic-site"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "creusot-diagnostic-site"
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
pub struct CreusotDiagnosticFinding {
    rule: CreusotDiagnosticRule,
    #[getter(copy)]
    disposition: Disposition,
    anchor: crate::objects::NodeAnchor,
    crate_name: String,
    context: String,
    span: FileSpan,
    snippet: String,
}

impl CreusotDiagnosticFinding {
    pub fn builder() -> CreusotDiagnosticFindingBuilder {
        CreusotDiagnosticFindingBuilder::default()
    }
}

impl Finding for CreusotDiagnosticFinding {
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
pub struct CreusotDiagnosticRecord {
    /// Stable probe rule identifier.
    #[getter(copy)]
    rule_id: CreusotDiagnosticRuleId,
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

impl CreusotDiagnosticRecord {
    /// Start a builder for this value.
    pub fn builder() -> CreusotDiagnosticRecordBuilder {
        CreusotDiagnosticRecordBuilder::default()
    }
}
