use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;
/// Whether a foreign error boundary was resolved to a concrete type or is
/// only a heuristic candidate. Lives here (not in `foreign_error_types`)
/// because [`crate::enricher::ErrorFlowEnricher`] — part of the always-on
/// `error_sites` enricher stack — sets it unconditionally; the
/// `foreign_error_types` etiquette only *consumes* it into findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForeignErrorRecordKind {
    Typed,
    Candidate,
}

impl ForeignErrorRecordKind {
    #[instrument(level = "debug", skip(self))]
    pub fn as_attr(self) -> &'static str {
        match self {
            Self::Typed => "typed",
            Self::Candidate => "candidate",
        }
    }

    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "typed" => Some(Self::Typed),
            "candidate" => Some(Self::Candidate),
            _ => None,
        }
    }
}

/// How an error enters or moves through control flow at this site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorSiteKind {
    /// question_mark.
    QuestionMark,
    /// map_err.
    MapErr,
    /// return_err.
    ReturnErr,
    /// if_let_err.
    IfLetErr,
    /// match_err.
    MatchErr,
    /// ok_or.
    OkOr,
}

impl ErrorSiteKind {
    /// Stable identifier string for IR attributes.
    #[instrument(level = "debug", skip(self))]
    pub fn as_attr(self) -> &'static str {
        match self {
            Self::QuestionMark => "question_mark",
            Self::MapErr => "map_err",
            Self::ReturnErr => "return_err",
            Self::IfLetErr => "if_let_err",
            Self::MatchErr => "match_err",
            Self::OkOr => "ok_or",
        }
    }

    /// Parse from the stable identifier string.
    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "question_mark" => Some(Self::QuestionMark),
            "map_err" => Some(Self::MapErr),
            "return_err" => Some(Self::ReturnErr),
            "if_let_err" => Some(Self::IfLetErr),
            "match_err" => Some(Self::MatchErr),
            "ok_or" => Some(Self::OkOr),
            _ => None,
        }
    }

    /// Preferred wrap is `.map_err` into a typed constructor that keeps the
    /// foreign error (and optional caller context: path, span). That site
    /// kind is not itself a chain break — only an un-preserved converter is.
    #[instrument(level = "debug", skip(self))]
    pub fn map_err_is_chain_break(self, chain_preserved: bool) -> bool {
        matches!(self, Self::MapErr) && !chain_preserved
    }
}

impl Display for ErrorSiteKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::QuestionMark => write!(f, "ERROR-SITE-QUESTION-MARK"),
            Self::MapErr => write!(f, "ERROR-SITE-MAP-ERR"),
            Self::ReturnErr => write!(f, "ERROR-SITE-RETURN-ERR"),
            Self::IfLetErr => write!(f, "ERROR-SITE-IF-LET-ERR"),
            Self::MatchErr => write!(f, "ERROR-SITE-MATCH-ERR"),
            Self::OkOr => write!(f, "ERROR-SITE-OK-OR"),
        }
    }
}

/// Partition bucket for an error site row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorOriginClass {
    /// An internal (crate-defined) error type.
    Internal,
    /// Any other item kind.
    Other,
    /// A graph edge, not a node.
    Edge,
}

impl Display for ErrorOriginClass {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Internal => write!(f, "ERROR-ORIGIN-INTERNAL"),
            Self::Other => write!(f, "ERROR-ORIGIN-OTHER"),
            Self::Edge => write!(f, "ERROR-ORIGIN-EDGE"),
        }
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct ErrorSiteRule {
    kind: ErrorSiteKind,
}

impl Rule for ErrorSiteRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        match self.kind {
            ErrorSiteKind::QuestionMark => "ERROR-SITE-QUESTION-MARK",
            ErrorSiteKind::MapErr => "ERROR-SITE-MAP-ERR",
            ErrorSiteKind::ReturnErr => "ERROR-SITE-RETURN-ERR",
            ErrorSiteKind::IfLetErr => "ERROR-SITE-IF-LET-ERR",
            ErrorSiteKind::MatchErr => "ERROR-SITE-MATCH-ERR",
            ErrorSiteKind::OkOr => "ERROR-SITE-OK-OR",
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "error_sites"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Control-flow site where a Result error is propagated, converted, returned, or constructed"
    }
}

#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct ErrorSiteMarker {
    anchor: crate::objects::NodeAnchor,
}

impl Marker for ErrorSiteMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "error-site"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "error-site"
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
pub struct ErrorSiteFinding {
    rule: ErrorSiteRule,
    #[getter(copy)]
    disposition: Disposition,
    anchor: crate::objects::NodeAnchor,
    crate_name: String,
    #[getter(copy)]
    kind: ErrorSiteKind,
    context: String,
    span: FileSpan,
    source_snippet: String,
    site_snippet: String,
    #[getter(copy)]
    origin_class: ErrorOriginClass,
    origin_detail: String,
    rationale: String,
}

impl ErrorSiteFinding {
    /// Start a builder for this value.
    pub fn builder() -> ErrorSiteFindingBuilder {
        ErrorSiteFindingBuilder::default()
    }
}

impl Finding for ErrorSiteFinding {
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
        sink.field("site_kind", &self.kind.to_string());
        sink.field("context", &self.context);
        sink.field("file", &self.span.file().display().to_string());
        sink.field("line", &self.span.line().to_string());
        sink.field("source_snippet", &self.source_snippet);
        sink.field("site_snippet", &self.site_snippet);
        sink.field("origin_class", &self.origin_class.to_string());
        sink.field("origin_detail", &self.origin_detail);
        sink.field("rationale", &self.rationale);
        sink.snippet(&self.site_snippet);
    }
}

/// Raw scan row used while building IR nodes.
#[derive(Debug, Clone, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct ErrorSiteRecord {
    #[getter(copy)]
    kind: ErrorSiteKind,
    context: String,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    source_snippet: String,
    site_snippet: String,
}

impl ErrorSiteRecord {
    /// Start a builder for this value.
    pub fn builder() -> ErrorSiteRecordBuilder {
        ErrorSiteRecordBuilder::default()
    }
}

/// Intermediate scan input for partition logic.
#[derive(Debug, Clone, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct ErrorSiteScanRow {
    /// Cargo package name.
    crate_name: String,
    /// Error-site kind (`?`, `map_err`, …).
    #[getter(copy)]
    kind: ErrorSiteKind,
    /// Qualified name or extra locator for this site.
    context: String,
    /// Source file path, usually crate-relative.
    file: PathBuf,
    /// Source line number (1-based), when known.
    #[getter(copy)]
    line: u32,
    /// Snippet of the originating expression.
    source_snippet: String,
    /// Snippet of the conversion site.
    site_snippet: String,
}

impl ErrorSiteScanRow {
    /// Start a builder for this value.
    pub fn builder() -> ErrorSiteScanRowBuilder {
        ErrorSiteScanRowBuilder::default()
    }
}

/// Count findings by site kind for summaries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ErrorSiteKindCounts {
    pub question_mark: usize,
    pub map_err: usize,
    pub return_err: usize,
    pub if_let_err: usize,
    pub match_err: usize,
    pub ok_or: usize,
}

impl ErrorSiteKindCounts {
    #[instrument(level = "trace", skip(self))]
    pub fn total(&self) -> usize {
        self.question_mark
            + self.map_err
            + self.return_err
            + self.if_let_err
            + self.match_err
            + self.ok_or
    }
}

/// Count partitioned rows by origin class.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ErrorOriginClassCounts {
    pub internal: usize,
    pub other: usize,
    pub edge: usize,
}
