use std::collections::BTreeMap;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use crate::etiquettes::error_sites::{
    ErrorOriginClass, ErrorSiteKind, ForeignTypeConfidence, PartitionedErrorSiteRow,
};
use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;
/// Partitioned findings for one crate.
#[derive(Debug, Clone, PartialEq, Eq, derive_new::new, derive_getters::Getters)]
pub struct ErrorSitePartitionReport {
    /// Cargo package name.
    crate_name: String,
    /// Findings produced by assessors in this session.
    findings: Vec<PartitionedErrorSiteRow>,
}

/// One site with an inferred std / third-party error type.
#[derive(Debug, Clone, PartialEq, Eq, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct ForeignErrorTypeRecord {
    /// Cargo package name.
    crate_name: String,
    /// Foreign error type named at this site.
    foreign_error_type: String,
    /// Stable probe rule identifier.
    rule_id: String,
    /// How confidently this site was classified as a foreign type.
    #[getter(copy)]
    confidence: ForeignTypeConfidence,
    /// Whether this site drops the `source()` chain.
    #[getter(copy)]
    chain_break: bool,
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

impl ForeignErrorTypeRecord {
    /// Start a builder for this value.
    pub fn builder() -> ForeignErrorTypeRecordBuilder {
        ForeignErrorTypeRecordBuilder::default()
    }
}

/// Inferred foreign error types for one crate.
#[derive(Debug, Clone, PartialEq, Eq, derive_new::new, derive_getters::Getters)]
pub struct ForeignErrorTypeReport {
    /// Cargo package name.
    crate_name: String,
    /// Findings produced by assessors in this session.
    findings: Vec<ForeignErrorTypeRecord>,
}

/// Per foreign error type rollup row.
#[derive(Debug, Clone, PartialEq, Eq, derive_new::new, derive_getters::Getters)]
pub struct ForeignErrorTypeSummaryRow {
    foreign_error_type: String,
    #[getter(copy)]
    chain_breaks: usize,
    #[getter(copy)]
    total: usize,
}

/// Workspace rollup for inferred foreign error types.
#[derive(Debug, Clone, PartialEq, Eq, derive_new::new, derive_getters::Getters)]
pub struct WorkspaceForeignErrorTypeSummary {
    /// How many sites were inferred rather than annotated.
    #[getter(copy)]
    inferred_sites: usize,
    /// Places the `source()` chain is dropped.
    #[getter(copy)]
    chain_breaks: usize,
    /// Type names collected for this row.
    types: Vec<ForeignErrorTypeSummaryRow>,
}

/// Build workspace foreign error type summary.
#[instrument(level = "debug", skip(reports))]
pub fn build_workspace_foreign_error_type_summary(
    reports: &[ForeignErrorTypeReport],
) -> WorkspaceForeignErrorTypeSummary {
    let mut by_type: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut inferred_sites = 0usize;
    let mut chain_breaks = 0usize;

    for report in reports {
        for finding in report.findings() {
            inferred_sites += 1;
            if finding.chain_break() {
                chain_breaks += 1;
            }
            let entry = by_type
                .entry(finding.foreign_error_type().clone())
                .or_default();
            entry.1 += 1;
            if finding.chain_break() {
                entry.0 += 1;
            }
        }
    }

    let mut types: Vec<ForeignErrorTypeSummaryRow> = by_type
        .into_iter()
        .map(|(foreign_error_type, (chain_break_count, total))| {
            ForeignErrorTypeSummaryRow::new(foreign_error_type, chain_break_count, total)
        })
        .collect();
    types.sort_by(|a, b| {
        b.chain_breaks()
            .cmp(&a.chain_breaks())
            .then(b.total().cmp(&a.total()))
            .then(a.foreign_error_type().cmp(b.foreign_error_type()))
    });

    WorkspaceForeignErrorTypeSummary::new(inferred_sites, chain_breaks, types)
}

/// Re-exported from `error_sites`, which sets this attribute unconditionally
/// via the shared `ErrorFlowEnricher` regardless of whether this etiquette
/// (`foreign_error_types`) is itself enabled.
pub use crate::etiquettes::error_sites::ForeignErrorRecordKind;

#[derive(Debug, Clone, derive_new::new)]
pub struct ForeignErrorTypeRule {
    #[new(into)]
    rule_id: String,
}

impl Rule for ForeignErrorTypeRule {
    fn id(&self) -> &str {
        &self.rule_id
    }

    fn category(&self) -> &str {
        "foreign_error_types"
    }

    fn description(&self) -> &str {
        "Inferred std or third-party error type at a foreign error boundary"
    }
}

#[derive(Debug, Clone)]
pub struct ForeignErrorCandidateRule;

impl Rule for ForeignErrorCandidateRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "FOREIGN-ERROR-CANDIDATE"
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "foreign_error_types"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Partitioned error site classified as other or edge foreign-error candidate"
    }
}

#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct ForeignErrorTypeMarker {
    anchor: crate::objects::NodeAnchor,
}

impl Marker for ForeignErrorTypeMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "foreign-error-type"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "foreign-error-type"
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
pub struct ForeignErrorTypeFinding {
    rule: ForeignErrorTypeRule,
    #[getter(copy)]
    disposition: Disposition,
    anchor: crate::objects::NodeAnchor,
    #[getter(copy)]
    record_kind: ForeignErrorRecordKind,
    crate_name: String,
    foreign_error_type: String,
    inference_rule_id: String,
    #[getter(copy)]
    confidence: ForeignTypeConfidence,
    #[getter(copy)]
    chain_break: bool,
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

impl ForeignErrorTypeFinding {
    /// Start a builder for this value.
    pub fn builder() -> ForeignErrorTypeFindingBuilder {
        ForeignErrorTypeFindingBuilder::default()
    }
}

impl Finding for ForeignErrorTypeFinding {
    #[instrument(level = "trace", skip(self))]
    fn rule(&self) -> &dyn Rule {
        if self.record_kind == ForeignErrorRecordKind::Candidate {
            static CANDIDATE_RULE: ForeignErrorCandidateRule = ForeignErrorCandidateRule;
            &CANDIDATE_RULE
        } else {
            &self.rule
        }
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
        sink.field("record_kind", &self.record_kind.to_string());
        sink.field("foreign_error_type", &self.foreign_error_type);
        sink.field("inference_rule_id", &self.inference_rule_id);
        sink.field("confidence", &self.confidence.to_string());
        sink.field("chain_break", &self.chain_break.to_string());
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

impl Display for ForeignErrorRecordKind {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Typed => write!(f, "typed"),
            Self::Candidate => write!(f, "candidate"),
        }
    }
}
