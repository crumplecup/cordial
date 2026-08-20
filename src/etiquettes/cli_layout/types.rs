use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;

/// Mechanical CLI layout violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CliLayoutId {
    /// Clap or error types live only on the binary side.
    Island001,
    /// Library clap type missing `act`, not handing off to each nested clap type, or dispatch in a free function.
    Act001,
    /// `main` does more than parse, `act`, and miette.
    Main001,
}

impl CliLayoutId {
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Island001 => "CLI-ISLAND-001",
            Self::Act001 => "CLI-ACT-001",
            Self::Main001 => "CLI-MAIN-001",
        }
    }
}

impl Display for CliLayoutId {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

/// One CLI-layout scan row (crate-level, before IR).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliLayoutRecord {
    pub crate_name: String,
    pub rule_id: CliLayoutId,
    pub context: String,
    pub file: PathBuf,
    pub line: u32,
    pub snippet: String,
}

#[derive(Debug, Clone, derive_new::new)]
pub struct CliLayoutRule {
    rule_id: CliLayoutId,
}

impl Rule for CliLayoutRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "cli_layout"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Clap types and dispatch belong in the library; main is parse + act + miette"
    }
}

#[derive(Debug, Clone)]
pub struct CliLayoutMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for CliLayoutMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "cli-layout-site"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "cli-layout-site"
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
pub struct CliLayoutFinding {
    pub rule: CliLayoutRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub context: String,
    pub span: FileSpan,
    pub snippet: String,
}

impl Finding for CliLayoutFinding {
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
    }
}
