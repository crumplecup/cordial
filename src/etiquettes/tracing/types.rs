use std::fmt::{Display, Formatter, Result as FmtResult};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

/// How a discovered function is categorized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionKind {
    Free,
    InherentMethod,
    TraitImplMethod,
}

impl Display for FunctionKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Free => write!(f, "free"),
            Self::InherentMethod => write!(f, "inherent"),
            Self::TraitImplMethod => write!(f, "trait_impl"),
        }
    }
}

/// Rust visibility rendered for reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisibilityLabel {
    Public,
    PubCrate,
    PubSuper,
    PubInPath(String),
    Private,
}

impl Display for VisibilityLabel {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Public => write!(f, "pub"),
            Self::PubCrate => write!(f, "pub(crate)"),
            Self::PubSuper => write!(f, "pub(super)"),
            Self::PubInPath(path) => write!(f, "pub({path})"),
            Self::Private => write!(f, "private"),
        }
    }
}

/// One discovered function before IR materialization.
#[derive(Debug, Clone)]
pub struct FunctionRecord {
    pub crate_name: String,
    pub qualified_name: String,
    pub kind: FunctionKind,
    pub visibility: VisibilityLabel,
    pub file: String,
    pub line: u32,
    pub instrumented: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum TracingRuleKind {
    MissingInstrument,
}

impl TracingRuleKind {
    pub fn rule_id(self) -> &'static str {
        match self {
            Self::MissingInstrument => "TRACING-MISSING-INSTRUMENT",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TracingRule {
    pub kind: TracingRuleKind,
}

impl TracingRule {
    pub fn new(kind: TracingRuleKind) -> Self {
        Self { kind }
    }
}

impl Rule for TracingRule {
    fn id(&self) -> &str {
        self.kind.rule_id()
    }

    fn category(&self) -> &str {
        "tracing"
    }

    fn description(&self) -> &str {
        "Public or crate-visible function missing `#[instrument]`"
    }
}

#[derive(Debug, Clone)]
pub struct TracingMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for TracingMarker {
    fn probe(&self) -> &str {
        "missing-instrument"
    }

    fn label(&self) -> &str {
        "missing-instrument"
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    fn span(&self) -> Option<&dyn SourceSpan> {
        None
    }
}

#[derive(Debug, Clone)]
pub struct TracingFinding {
    pub rule: TracingRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub qualified_name: String,
    pub kind: FunctionKind,
    pub visibility: VisibilityLabel,
    pub span: FileSpan,
}

impl Finding for TracingFinding {
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
        sink.field("kind", &self.rule.id());
        sink.field("context", &self.qualified_name);
        sink.field("qualified_name", &self.qualified_name);
        sink.field("function_kind", &self.kind);
        sink.field("visibility", &self.visibility);
        sink.field("file", &self.span.file.display().to_string());
        sink.field("line", &self.span.line.to_string());
    }
}
