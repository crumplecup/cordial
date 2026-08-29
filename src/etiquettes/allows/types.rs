use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;
/// Stable rule identifier for an allow-attribute probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AllowRuleId {
    /// Any `#[allow(...)]` or `#![allow(...)]` attribute.
    Attr001,
    /// Verus `vstd` / `verus_builtin` import allow missing `reason = "..."`.
    VerusReason001,
}

impl AllowRuleId {
    /// Stable string form of this value.
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Attr001 => "ALLOW-ATTR-001",
            Self::VerusReason001 => "ALLOW-VERUS-REASON-001",
        }
    }

    /// Parse from the stable identifier string.
    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "ALLOW-ATTR-001" => Some(Self::Attr001),
            "ALLOW-VERUS-REASON-001" => Some(Self::VerusReason001),
            _ => None,
        }
    }
}

impl Display for AllowRuleId {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct AllowRule {
    rule_id: AllowRuleId,
}

impl Rule for AllowRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "allows"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        match self.rule_id {
            AllowRuleId::Attr001 => {
                "`#[allow(...)]` or `#![allow(...)]` attribute suppressing compiler warnings"
            }
            AllowRuleId::VerusReason001 => {
                "Verus `vstd`/`verus_builtin` import `#[allow]` must include `reason = \"...\"`"
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct AllowMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for AllowMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "allow-site"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "allow-site"
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
pub struct AllowFinding {
    pub rule: AllowRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub context: String,
    pub span: FileSpan,
    pub snippet: String,
}

impl Finding for AllowFinding {
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
#[derive(Debug, Clone)]
pub struct AllowSiteRecord {
    /// Stable probe rule identifier.
    pub rule_id: AllowRuleId,
    /// Qualified name or extra locator for this site.
    pub context: String,
    /// Source file path, usually crate-relative.
    pub file: PathBuf,
    /// Source line number (1-based), when known.
    pub line: u32,
    /// Source snippet captured at the site.
    pub snippet: String,
}
