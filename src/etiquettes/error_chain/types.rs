use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;
/// Probe rule for preserved foreign error chains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorChainProbeId {
    /// Wrapper struct carries foreign error in a `source` field (not `String`).
    WrapperSourceField001,
    /// Umbrella `ErrorKind` variant payload is a wrapper type (not `String`).
    KindWrapperPayload001,
    /// `impl From<ForeignError> for Wrapper` (or umbrella) bridge.
    FromBridge001,
    /// Foreign `Result` propagated with `?` without stringifying `map_err`.
    PreservedQuestionMark001,
    /// Foreign `Result` converted via chain-preserving `map_err` then `?`
    /// (typed constructor / `From` / forwarding `err` plus caller context).
    /// This is the preferred wrap, not a chain break.
    PreservedMapErr001,
}

impl ErrorChainProbeId {
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WrapperSourceField001 => "ERROR-CHAIN-WRAPPER-SOURCE-001",
            Self::KindWrapperPayload001 => "ERROR-CHAIN-KIND-WRAPPER-PAYLOAD-001",
            Self::FromBridge001 => "ERROR-CHAIN-FROM-BRIDGE-001",
            Self::PreservedQuestionMark001 => "ERROR-CHAIN-PRESERVED-QUESTION-MARK-001",
            Self::PreservedMapErr001 => "ERROR-CHAIN-PRESERVED-MAP-ERR-001",
        }
    }

    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "ERROR-CHAIN-WRAPPER-SOURCE-001" => Some(Self::WrapperSourceField001),
            "ERROR-CHAIN-KIND-WRAPPER-PAYLOAD-001" => Some(Self::KindWrapperPayload001),
            "ERROR-CHAIN-FROM-BRIDGE-001" => Some(Self::FromBridge001),
            "ERROR-CHAIN-PRESERVED-QUESTION-MARK-001" => Some(Self::PreservedQuestionMark001),
            "ERROR-CHAIN-PRESERVED-MAP-ERR-001" => Some(Self::PreservedMapErr001),
            _ => None,
        }
    }
}

impl Display for ErrorChainProbeId {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct ErrorChainRule {
    pub rule_id: ErrorChainProbeId,
}

impl ErrorChainRule {
    #[instrument(level = "debug", skip(rule_id), ret)]
    pub fn new(rule_id: ErrorChainProbeId) -> Self {
        Self { rule_id }
    }
}

impl Rule for ErrorChainRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "error_chain"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Positive pattern preserving a foreign error chain through wrapping or propagation"
    }
}

#[derive(Debug, Clone)]
pub struct ErrorChainMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for ErrorChainMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "error-chain"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "error-chain"
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
pub struct ErrorChainFinding {
    pub rule: ErrorChainRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub context: String,
    pub span: FileSpan,
    pub snippet: String,
    pub foreign_error_type: Option<String>,
}

impl Finding for ErrorChainFinding {
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
        sink.field(
            "foreign_error_type",
            &self.foreign_error_type.clone().unwrap_or_default(),
        );
        sink.field("context", &self.context);
        sink.field("file", &self.span.file.display().to_string());
        sink.field("line", &self.span.line.to_string());
        sink.field("snippet", &self.snippet);
        sink.snippet(&self.snippet);
    }
}

/// Raw scan row used while building IR nodes.
#[derive(Debug, Clone)]
pub struct ErrorChainRecord {
    pub rule_id: ErrorChainProbeId,
    pub context: String,
    pub file: PathBuf,
    pub line: u32,
    pub snippet: String,
    pub foreign_error_type: Option<String>,
}

/// Count findings by probe rule.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ErrorChainProbeCounts {
    pub wrapper_source: usize,
    pub kind_wrapper_payload: usize,
    pub from_bridge: usize,
    pub preserved_question_mark: usize,
    pub preserved_map_err: usize,
}

impl ErrorChainProbeCounts {
    #[instrument(level = "trace", skip(self))]
    pub fn total(&self) -> usize {
        self.wrapper_source
            + self.kind_wrapper_payload
            + self.from_bridge
            + self.preserved_question_mark
            + self.preserved_map_err
    }

    #[instrument(level = "trace", skip(self))]
    pub fn preserved_propagation(&self) -> usize {
        self.preserved_question_mark + self.preserved_map_err
    }

    #[instrument(level = "trace", skip(self))]
    pub fn infrastructure(&self) -> usize {
        self.wrapper_source + self.kind_wrapper_payload + self.from_bridge
    }
}

#[instrument(level = "debug", skip(records))]
pub fn probe_counts(records: &[ErrorChainRecord]) -> ErrorChainProbeCounts {
    let mut counts = ErrorChainProbeCounts::default();
    for record in records {
        match record.rule_id {
            ErrorChainProbeId::WrapperSourceField001 => counts.wrapper_source += 1,
            ErrorChainProbeId::KindWrapperPayload001 => counts.kind_wrapper_payload += 1,
            ErrorChainProbeId::FromBridge001 => counts.from_bridge += 1,
            ErrorChainProbeId::PreservedQuestionMark001 => counts.preserved_question_mark += 1,
            ErrorChainProbeId::PreservedMapErr001 => counts.preserved_map_err += 1,
        }
    }
    counts
}
