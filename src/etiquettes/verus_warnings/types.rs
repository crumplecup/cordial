use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;

/// Stable rule identifier for a Verus compiler warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VerusWarningRuleId {
    /// A `warning:` diagnostic from the Verus rustc fork.
    Warning001,
}

impl VerusWarningRuleId {
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warning001 => "VERUS-WARNING-001",
        }
    }

    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "VERUS-WARNING-001" => Some(Self::Warning001),
            _ => None,
        }
    }
}

impl Display for VerusWarningRuleId {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct VerusWarningRule {
    rule_id: VerusWarningRuleId,
}

impl Rule for VerusWarningRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "verus_warnings"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Verus compiler warning — rustc/clippy never see this diagnostic"
    }
}

#[derive(Debug, Clone)]
pub struct VerusWarningMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for VerusWarningMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "verus-warning-site"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "verus-warning-site"
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

#[derive(Debug, Clone)]
pub struct VerusWarningFinding {
    pub rule: VerusWarningRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub context: String,
    pub span: FileSpan,
    pub snippet: String,
}

impl Finding for VerusWarningFinding {
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
        sink.field("file", &self.span.file.display().to_string());
        sink.field("line", &self.span.line.to_string());
        sink.field("snippet", &self.snippet);
        sink.snippet(&self.snippet);
    }
}

/// Raw scan row used while building IR nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerusWarningRecord {
    pub rule_id: VerusWarningRuleId,
    pub context: String,
    pub file: PathBuf,
    pub line: u32,
    pub snippet: String,
}
