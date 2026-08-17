use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

/// Stable rule identifier for an allow-attribute probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AllowRuleId {
    /// Any `#[allow(...)]` or `#![allow(...)]` attribute.
    Attr001,
}

impl AllowRuleId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Attr001 => "ALLOW-ATTR-001",
        }
    }

    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "ALLOW-ATTR-001" => Some(Self::Attr001),
            _ => None,
        }
    }
}

impl Display for AllowRuleId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct AllowRule {
    pub rule_id: AllowRuleId,
}

impl AllowRule {
    pub fn new(rule_id: AllowRuleId) -> Self {
        Self { rule_id }
    }
}

impl Rule for AllowRule {
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    fn category(&self) -> &str {
        "allows"
    }

    fn description(&self) -> &str {
        "`#[allow(...)]` or `#![allow(...)]` attribute suppressing compiler warnings"
    }
}

#[derive(Debug, Clone)]
pub struct AllowMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for AllowMarker {
    fn probe(&self) -> &str {
        "allow-site"
    }

    fn label(&self) -> &str {
        "allow-site"
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

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
    pub rule_id: AllowRuleId,
    pub context: String,
    pub file: PathBuf,
    pub line: u32,
    pub snippet: String,
}
