use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;

/// Stable rule identifier for a leftover stdio macro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrintRuleId {
    /// Leftover `println!`.
    Println,
    /// Leftover `eprintln!`.
    Eprintln,
    /// Leftover `print!`.
    Print,
    /// Leftover `eprint!`.
    Eprint,
    /// Leftover `dbg!`.
    Dbg,
}

impl PrintRuleId {
    /// Stable string form of this value.
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Println => "TRACING-STD-PRINTLN",
            Self::Eprintln => "TRACING-STD-EPRINTLN",
            Self::Print => "TRACING-STD-PRINT",
            Self::Eprint => "TRACING-STD-EPRINT",
            Self::Dbg => "TRACING-STD-DBG",
        }
    }

    /// Parse from the stable identifier string.
    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "TRACING-STD-PRINTLN" => Some(Self::Println),
            "TRACING-STD-EPRINTLN" => Some(Self::Eprintln),
            "TRACING-STD-PRINT" => Some(Self::Print),
            "TRACING-STD-EPRINT" => Some(Self::Eprint),
            "TRACING-STD-DBG" => Some(Self::Dbg),
            _ => None,
        }
    }

    /// Whether `id` is a leftover-stdio rule (`TRACING-STD-*`).
    #[instrument(level = "debug")]
    pub fn is_print_rule(id: &str) -> bool {
        Self::from_attr(id).is_some()
    }

    /// Macro token captured at the site (`println!`, `dbg!`, …).
    #[instrument(level = "trace", skip(self))]
    pub fn snippet(self) -> &'static str {
        match self {
            Self::Println => "println!",
            Self::Eprintln => "eprintln!",
            Self::Print => "print!",
            Self::Eprint => "eprint!",
            Self::Dbg => "dbg!",
        }
    }

    #[instrument(level = "debug", skip(self))]
    fn description(self) -> &'static str {
        match self {
            Self::Println => {
                "Replace leftover `println!` with `tracing::info!` or `tracing::debug!`"
            }
            Self::Eprintln => {
                "Replace leftover `eprintln!` with `tracing::warn!` or `tracing::error!`"
            }
            Self::Print => "Replace leftover `print!` with `tracing::info!` or `tracing::debug!`",
            Self::Eprint => "Replace leftover `eprint!` with `tracing::warn!` or `tracing::error!`",
            Self::Dbg => "Replace leftover `dbg!` with `tracing::debug!`",
        }
    }
}

impl Display for PrintRuleId {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct PrintRule {
    rule_id: PrintRuleId,
}

impl Rule for PrintRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "tracing"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        self.rule_id.description()
    }
}

pub const PRINT_SITE_LABEL: &str = "tracing-print-site";

#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct PrintMarker {
    anchor: crate::objects::NodeAnchor,
}

impl Marker for PrintMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        PRINT_SITE_LABEL
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        PRINT_SITE_LABEL
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
pub struct PrintFinding {
    rule: PrintRule,
    #[getter(copy)]
    disposition: Disposition,
    anchor: crate::objects::NodeAnchor,
    crate_name: String,
    context: String,
    span: FileSpan,
    snippet: String,
}

impl PrintFinding {
    /// Start a builder for this value.
    pub fn builder() -> PrintFindingBuilder {
        PrintFindingBuilder::default()
    }
}

impl Finding for PrintFinding {
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
        sink.field("rule", &self.rule.rule_id);
        sink.field("context", &self.context);
        sink.field("file", &self.span.file().display().to_string());
        sink.field("line", &self.span.line().to_string());
        sink.field("snippet", &self.snippet);
        sink.snippet(&self.snippet);
    }
}

/// Raw scan row used while building IR nodes.
#[derive(Debug, Clone, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct PrintSiteRecord {
    /// Stable probe rule identifier.
    #[getter(copy)]
    rule_id: PrintRuleId,
    /// Qualified module path for this site.
    context: String,
    /// Source file path, usually crate-relative.
    file: PathBuf,
    /// Source line number (1-based), when known.
    #[getter(copy)]
    line: u32,
    /// Captured macro name (`println!`, `print!`, `dbg!`, …).
    snippet: String,
}

impl PrintSiteRecord {
    /// Start a builder for this value.
    pub fn builder() -> PrintSiteRecordBuilder {
        PrintSiteRecordBuilder::default()
    }
}
