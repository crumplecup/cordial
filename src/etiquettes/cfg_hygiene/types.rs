use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;
/// Stable rule identifier for a cfg-hygiene finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CfgHygieneRuleId {
    /// A `cfg(X)`/`cfg_attr(X, ...)` names an `X` that isn't declared in any
    /// check-cfg source reachable by the crate it appears in (rustc's own
    /// built-in vocabulary, Cargo's `test`/`feature`/`docsrs`, this crate's
    /// own `build.rs`/`Cargo.toml [lints.rust]`, or the workspace's
    /// `[workspace.lints.rust]` if this crate opts in via
    /// `[lints] workspace = true`).
    UnexpectedCfg001,
    /// A crate registered in `cordial.toml`'s `[cfg_hygiene] crate_verifier`
    /// table uses a *different* verifier's cfg name than its own configured
    /// identity — the real gap a workspace-wide check-cfg union creates and
    /// can never self-detect. Only checked for crates actually listed in
    /// that table; see [`super::declared::declared_names_for_crate`]'s doc.
    CfgVerifierMismatch001,
}

impl CfgHygieneRuleId {
    /// Stable string form of this value.
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnexpectedCfg001 => "UNEXPECTED-CFG-001",
            Self::CfgVerifierMismatch001 => "CFG-VERIFIER-MISMATCH-001",
        }
    }

    /// Parse from the stable identifier string.
    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "UNEXPECTED-CFG-001" => Some(Self::UnexpectedCfg001),
            "CFG-VERIFIER-MISMATCH-001" => Some(Self::CfgVerifierMismatch001),
            _ => None,
        }
    }
}

impl Display for CfgHygieneRuleId {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct CfgHygieneRule {
    rule_id: CfgHygieneRuleId,
}

impl Rule for CfgHygieneRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "cfg_hygiene"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        match self.rule_id {
            CfgHygieneRuleId::UnexpectedCfg001 => {
                "cfg name not declared in any check-cfg source reachable by this crate"
            }
            CfgHygieneRuleId::CfgVerifierMismatch001 => {
                "crate uses a verifier cfg name that isn't its own configured identity"
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct CfgHygieneMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for CfgHygieneMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "cfg-hygiene-site"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "cfg-hygiene-site"
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
pub struct CfgHygieneFinding {
    pub rule: CfgHygieneRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub cfg_name: String,
    pub context: String,
    pub span: FileSpan,
    pub snippet: String,
}

impl Finding for CfgHygieneFinding {
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
        sink.field("cfg_name", &self.cfg_name);
        sink.field("context", &self.context);
        sink.field("file", &self.span.file.display().to_string());
        sink.field("line", &self.span.line.to_string());
        sink.field("snippet", &self.snippet);
        sink.snippet(&self.snippet);
    }
}

/// Raw scan row used while building IR nodes: one per flagged cfg-name
/// occurrence (an occurrence that mentions several names, e.g.
/// `any(kani, creusot)`, can produce more than one record).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfgHygieneSiteRecord {
    /// Stable probe rule identifier.
    pub rule_id: CfgHygieneRuleId,
    /// Cfg predicate name that is undeclared or mismatched.
    pub cfg_name: String,
    /// Qualified name or extra locator for this site.
    pub context: String,
    /// Source file path, usually crate-relative.
    pub file: PathBuf,
    /// Source line number (1-based), when known.
    pub line: u32,
    /// Source snippet captured at the site.
    pub snippet: String,
}
