use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;
/// Category of panic or hard-fail site detected in source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PanicKind {
    Panic,
    Unreachable,
    Expect,
    Unwrap,
    CompileError,
}

impl PanicKind {
    #[instrument(level = "debug", skip(self))]
    pub fn rule_id(self) -> &'static str {
        match self {
            Self::Panic => "PANIC-SOURCE-PANIC",
            Self::Unreachable => "PANIC-SOURCE-UNREACHABLE",
            Self::Expect => "PANIC-SOURCE-EXPECT",
            Self::Unwrap => "PANIC-SOURCE-UNWRAP",
            Self::CompileError => "PANIC-SOURCE-COMPILE-ERROR",
        }
    }

    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "panic" => Some(Self::Panic),
            "unreachable" => Some(Self::Unreachable),
            "expect" => Some(Self::Expect),
            "unwrap" => Some(Self::Unwrap),
            "compile_error" => Some(Self::CompileError),
            _ => None,
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn as_attr(self) -> &'static str {
        match self {
            Self::Panic => "panic",
            Self::Unreachable => "unreachable",
            Self::Expect => "expect",
            Self::Unwrap => "unwrap",
            Self::CompileError => "compile_error",
        }
    }
}

impl Display for PanicKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.rule_id())
    }
}

#[derive(Debug, Clone)]
pub struct PanicRule {
    pub kind: PanicKind,
}

impl PanicRule {
    #[instrument(level = "debug", skip(kind), ret)]
    pub fn new(kind: PanicKind) -> Self {
        Self { kind }
    }
}

impl Rule for PanicRule {
    fn id(&self) -> &str {
        self.kind.rule_id()
    }

    fn category(&self) -> &str {
        "panics"
    }

    fn description(&self) -> &str {
        "Non-Result failure site: panic, unreachable, expect, unwrap, or compile_error. \
         Library code should return internal error types; binary and test code should use miette"
    }
}

/// Marker emitted by [`super::probe::PanicSiteProbe`].
#[derive(Debug, Clone)]
pub struct PanicMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for PanicMarker {
    fn probe(&self) -> &str {
        "panic-site"
    }

    fn label(&self) -> &str {
        "panic-site"
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    fn span(&self) -> Option<&dyn SourceSpan> {
        None
    }
}

/// Assessed panic-site finding.
#[derive(Debug, Clone)]
pub struct PanicFinding {
    pub rule: PanicRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub context: String,
    pub span: FileSpan,
    pub snippet: String,
    pub surface: crate::plugin::ErrorSurface,
    pub checklist: bool,
}

impl Finding for PanicFinding {
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
        sink.field("kind", &self.rule.kind);
        sink.field("surface", &self.surface);
        sink.field("context", &self.context);
        sink.field("file", &self.span.file.display().to_string());
        sink.field("line", &self.span.line.to_string());
        sink.field("snippet", &self.snippet);
        sink.field("checklist", &self.checklist.to_string());
        sink.snippet(&self.snippet);
    }
}

/// Raw scan row used while building IR nodes.
#[derive(Debug, Clone)]
pub struct PanicSiteRecord {
    pub kind: PanicKind,
    pub context: String,
    pub file: PathBuf,
    pub line: u32,
    pub snippet: String,
    pub cfg_test: bool,
}
