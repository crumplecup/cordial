use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;

/// Stable rule identifier for a glob import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GlobImportRuleId {
    /// A `*` in a `use` tree (`use foo::*` or `use foo::{bar, *}`).
    Import001,
}

impl GlobImportRuleId {
    /// Stable string form of this value.
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Import001 => "GLOB-IMPORT-001",
        }
    }

    /// Parse from the stable identifier string.
    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "GLOB-IMPORT-001" => Some(Self::Import001),
            _ => None,
        }
    }
}

impl Display for GlobImportRuleId {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct GlobImportRule {
    rule_id: GlobImportRuleId,
}

impl Rule for GlobImportRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "glob_imports"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Glob `use` (`foo::*`, including `super::*`) — replace with explicit imports"
    }
}

#[derive(Debug, Clone)]
pub struct GlobImportMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for GlobImportMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "glob-import-site"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "glob-import-site"
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
pub struct GlobImportFinding {
    pub rule: GlobImportRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub context: String,
    pub span: FileSpan,
    pub snippet: String,
}

impl Finding for GlobImportFinding {
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
pub struct GlobImportSiteRecord {
    pub rule_id: GlobImportRuleId,
    pub context: String,
    pub file: PathBuf,
    pub line: u32,
    pub snippet: String,
}
