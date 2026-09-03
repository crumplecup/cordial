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
    /// panic.
    Panic,
    /// unreachable.
    Unreachable,
    /// expect.
    Expect,
    /// unwrap.
    Unwrap,
    /// compile_error.
    CompileError,
}

impl PanicKind {
    /// Rule id.
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

    /// Parse from the stable identifier string.
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

    /// Stable identifier string for IR attributes.
    #[instrument(level = "debug", skip(self))]
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
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.rule_id())
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct PanicRule {
    kind: PanicKind,
}

impl Rule for PanicRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.kind.rule_id()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "panics"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Non-Result failure site: panic, unreachable, expect, unwrap, or compile_error. \
         Library code should return internal error types; binary and test code should use miette"
    }
}

/// Marker emitted by [`super::probe::PanicSiteProbe`].
#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct PanicMarker {
    anchor: crate::objects::NodeAnchor,
}

impl Marker for PanicMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "panic-site"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "panic-site"
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

/// Assessed panic-site finding.
#[derive(Debug, Clone, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct PanicFinding {
    rule: PanicRule,
    #[getter(copy)]
    disposition: Disposition,
    anchor: crate::objects::NodeAnchor,
    crate_name: String,
    context: String,
    span: FileSpan,
    snippet: String,
    surface: crate::plugin::ErrorSurface,
    #[getter(copy)]
    checklist: bool,
}

impl PanicFinding {
    /// Start a builder for this finding.
    pub fn builder() -> PanicFindingBuilder {
        PanicFindingBuilder::default()
    }
}

impl Finding for PanicFinding {
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
        sink.field("kind", &self.rule.kind);
        sink.field("surface", &self.surface);
        sink.field("context", &self.context);
        sink.field("file", &self.span.file().display().to_string());
        sink.field("line", &self.span.line().to_string());
        sink.field("snippet", &self.snippet);
        sink.field("checklist", &self.checklist.to_string());
        sink.snippet(&self.snippet);
    }
}

/// Raw scan row used while building IR nodes.
#[derive(Debug, Clone, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct PanicSiteRecord {
    #[getter(copy)]
    kind: PanicKind,
    context: String,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    snippet: String,
    #[getter(copy)]
    cfg_test: bool,
}

impl PanicSiteRecord {
    /// Start a builder for this scan row.
    pub fn builder() -> PanicSiteRecordBuilder {
        PanicSiteRecordBuilder::default()
    }
}
