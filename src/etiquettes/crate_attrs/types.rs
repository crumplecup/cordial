use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;

/// Stable rule identifier for a crate-root attribute finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CrateAttrsRuleId {
    /// Library root is missing `#![forbid(unsafe_code)]`.
    ForbidUnsafe001,
    /// Library root is missing `#![warn(missing_docs)]` (deny/forbid also count).
    MissingDocs001,
}

impl CrateAttrsRuleId {
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ForbidUnsafe001 => "CRATE-FORBID-UNSAFE-001",
            Self::MissingDocs001 => "CRATE-MISSING-DOCS-001",
        }
    }

    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "CRATE-FORBID-UNSAFE-001" => Some(Self::ForbidUnsafe001),
            "CRATE-MISSING-DOCS-001" => Some(Self::MissingDocs001),
            _ => None,
        }
    }
}

impl Display for CrateAttrsRuleId {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct CrateAttrsRule {
    rule_id: CrateAttrsRuleId,
}

impl Rule for CrateAttrsRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "crate_attrs"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        match self.rule_id {
            CrateAttrsRuleId::ForbidUnsafe001 => {
                "library root is missing `#![forbid(unsafe_code)]`"
            }
            CrateAttrsRuleId::MissingDocs001 => {
                "library root is missing `#![warn(missing_docs)]` (deny/forbid also count)"
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct CrateAttrsMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for CrateAttrsMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "crate-attrs-site"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "crate-attrs-site"
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
pub struct CrateAttrsFinding {
    pub rule: CrateAttrsRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub context: String,
    pub span: FileSpan,
    pub snippet: String,
}

impl Finding for CrateAttrsFinding {
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
pub struct CrateAttrsSiteRecord {
    pub rule_id: CrateAttrsRuleId,
    pub context: String,
    pub file: PathBuf,
    pub line: u32,
    pub snippet: String,
}
