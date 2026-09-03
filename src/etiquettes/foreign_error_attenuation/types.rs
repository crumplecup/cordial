use std::collections::BTreeMap;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use crate::error::CordialResult;
use crate::etiquettes::error_sites::{ErrorSiteKind, ForeignTypeConfidence};
use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;
/// How a typed foreign error site aligns with chain-preservation probes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ForeignErrorHandlingClass {
    /// Chain Preserved.
    ChainPreserved,
    /// Chain Break.
    ChainBreak,
    /// Pending Infrastructure.
    PendingInfrastructure,
    /// Neutral.
    Neutral,
}

impl Display for ForeignErrorHandlingClass {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::ChainPreserved => write!(f, "ERROR-HANDLING-CHAIN-PRESERVED"),
            Self::ChainBreak => write!(f, "ERROR-HANDLING-CHAIN-BREAK"),
            Self::PendingInfrastructure => write!(f, "ERROR-HANDLING-PENDING-INFRA"),
            Self::Neutral => write!(f, "ERROR-HANDLING-NEUTRAL"),
        }
    }
}

/// Baked-in resolution strategy identifier for metrics rollups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorHandlingResolutionId {
    MaintainExemplar,
    ReplaceStringifyingMapErr,
    AddInfrastructureThenQuestionMark,
    ManualReview,
}

impl Display for ErrorHandlingResolutionId {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::MaintainExemplar => write!(f, "ERROR-RESOLUTION-MAINTAIN-EXEMPLAR"),
            Self::ReplaceStringifyingMapErr => {
                write!(f, "ERROR-RESOLUTION-REPLACE-STRINGIFY-MAP-ERR")
            }
            Self::AddInfrastructureThenQuestionMark => {
                write!(f, "ERROR-RESOLUTION-ADD-INFRA-THEN-QUESTION-MARK")
            }
            Self::ManualReview => write!(f, "ERROR-RESOLUTION-MANUAL-REVIEW"),
        }
    }
}

/// One foreign error site with positive/negative classification and resolution.
#[derive(Debug, Clone, PartialEq, Eq, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct ForeignErrorAttenuationRecord {
    crate_name: String,
    foreign_error_type: String,
    inference_rule_id: String,
    #[getter(copy)]
    confidence: ForeignTypeConfidence,
    #[getter(copy)]
    handling_class: ForeignErrorHandlingClass,
    #[getter(copy)]
    resolution_id: ErrorHandlingResolutionId,
    resolution: String,
    #[getter(copy)]
    kind: ErrorSiteKind,
    context: String,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    source_snippet: String,
    site_snippet: String,
    good_pattern: String,
    bad_pattern: String,
}

impl ForeignErrorAttenuationRecord {
    /// Start a builder for this value.
    pub fn builder() -> ForeignErrorAttenuationRecordBuilder {
        ForeignErrorAttenuationRecordBuilder::default()
    }
}

/// Attenuation report for one crate.
#[derive(Debug, Clone, PartialEq, Eq, derive_new::new, derive_getters::Getters)]
pub struct ForeignErrorAttenuationReport {
    /// Cargo package name.
    crate_name: String,
    /// Findings produced by assessors in this session.
    findings: Vec<ForeignErrorAttenuationRecord>,
}

/// Count rows by handling class.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ForeignErrorHandlingCounts {
    pub chain_preserved: usize,
    pub chain_break: usize,
    pub pending_infrastructure: usize,
    pub neutral: usize,
}

impl ForeignErrorAttenuationReport {
    /// Handling counts.
    #[instrument(level = "debug", skip(self))]
    pub fn handling_counts(&self) -> ForeignErrorHandlingCounts {
        let mut counts = ForeignErrorHandlingCounts::default();
        for finding in self.findings() {
            match finding.handling_class() {
                ForeignErrorHandlingClass::ChainPreserved => counts.chain_preserved += 1,
                ForeignErrorHandlingClass::ChainBreak => counts.chain_break += 1,
                ForeignErrorHandlingClass::PendingInfrastructure => {
                    counts.pending_infrastructure += 1;
                }
                ForeignErrorHandlingClass::Neutral => counts.neutral += 1,
            }
        }
        counts
    }

    /// Preservation rate.
    #[instrument(level = "debug", skip(self))]
    pub fn preservation_rate(&self) -> Option<f64> {
        let counts = self.handling_counts();
        let denominator = counts.chain_preserved + counts.chain_break;
        if denominator == 0 {
            return None;
        }
        Some(counts.chain_preserved as f64 / denominator as f64)
    }
}

/// Per foreign-type attenuation rollup.
#[derive(Debug, Clone, PartialEq, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct ForeignErrorAttenuationTypeRow {
    foreign_error_type: String,
    #[getter(copy)]
    chain_preserved: usize,
    #[getter(copy)]
    chain_breaks: usize,
    #[getter(copy)]
    pending_infrastructure: usize,
    #[getter(copy)]
    total: usize,
    #[getter(copy)]
    preservation_rate: Option<f64>,
    #[getter(copy)]
    primary_resolution_id: ErrorHandlingResolutionId,
}

impl ForeignErrorAttenuationTypeRow {
    /// Start a builder for this value.
    pub fn builder() -> ForeignErrorAttenuationTypeRowBuilder {
        ForeignErrorAttenuationTypeRowBuilder::default()
    }
}

/// Workspace attenuation metrics.
#[derive(Debug, Clone, PartialEq, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct WorkspaceForeignErrorAttenuationSummary {
    /// Sites that still have a typed foreign error.
    #[getter(copy)]
    typed_sites: usize,
    /// Sites that keep the foreign error in `source()`.
    #[getter(copy)]
    chain_preserved: usize,
    /// Places the `source()` chain is dropped.
    #[getter(copy)]
    chain_breaks: usize,
    /// Sites waiting on infrastructure before they can preserve the chain.
    #[getter(copy)]
    pending_infrastructure: usize,
    /// Sites that are neither a break nor a preservation.
    #[getter(copy)]
    neutral: usize,
    /// Fraction of typed sites that preserve the chain.
    #[getter(copy)]
    preservation_rate: Option<f64>,
    /// Sites still needing a typed wrap.
    #[getter(copy)]
    migration_backlog: usize,
    /// Type names collected for this row.
    types: Vec<ForeignErrorAttenuationTypeRow>,
    /// Counts keyed by resolution label.
    resolutions: BTreeMap<String, usize>,
}

impl WorkspaceForeignErrorAttenuationSummary {
    /// Start a builder for this value.
    pub fn builder() -> WorkspaceForeignErrorAttenuationSummaryBuilder {
        WorkspaceForeignErrorAttenuationSummaryBuilder::default()
    }
}

/// Build workspace foreign error attenuation summary.
#[instrument(level = "debug", skip(reports))]
pub fn build_workspace_foreign_error_attenuation_summary(
    reports: &[ForeignErrorAttenuationReport],
) -> CordialResult<WorkspaceForeignErrorAttenuationSummary> {
    let mut chain_preserved = 0usize;
    let mut chain_breaks = 0usize;
    let mut pending_infrastructure = 0usize;
    let mut neutral = 0usize;
    let mut by_type: BTreeMap<String, (usize, usize, usize, usize)> = BTreeMap::new();
    let mut resolutions: BTreeMap<String, usize> = BTreeMap::new();

    for report in reports {
        for finding in report.findings() {
            chain_preserved +=
                usize::from(finding.handling_class() == ForeignErrorHandlingClass::ChainPreserved);
            chain_breaks +=
                usize::from(finding.handling_class() == ForeignErrorHandlingClass::ChainBreak);
            pending_infrastructure += usize::from(
                finding.handling_class() == ForeignErrorHandlingClass::PendingInfrastructure,
            );
            neutral += usize::from(finding.handling_class() == ForeignErrorHandlingClass::Neutral);
            *resolutions
                .entry(finding.resolution_id().to_string())
                .or_default() += 1;

            let entry = by_type
                .entry(finding.foreign_error_type().clone())
                .or_default();
            entry.3 += 1;
            match finding.handling_class() {
                ForeignErrorHandlingClass::ChainPreserved => entry.0 += 1,
                ForeignErrorHandlingClass::ChainBreak => entry.1 += 1,
                ForeignErrorHandlingClass::PendingInfrastructure => entry.2 += 1,
                ForeignErrorHandlingClass::Neutral => {}
            }
        }
    }

    let typed_sites = chain_preserved + chain_breaks + pending_infrastructure + neutral;
    let preservation_rate = {
        let denominator = chain_preserved + chain_breaks;
        if denominator == 0 {
            None
        } else {
            Some(chain_preserved as f64 / denominator as f64)
        }
    };

    let mut types = Vec::new();
    for (foreign_error_type, (preserved, breaks, pending, total)) in by_type {
        let rate = {
            let denominator = preserved + breaks;
            if denominator == 0 {
                None
            } else {
                Some(preserved as f64 / denominator as f64)
            }
        };
        let primary_resolution_id = if preserved > 0 && breaks == 0 && pending == 0 {
            ErrorHandlingResolutionId::MaintainExemplar
        } else if breaks > 0 {
            ErrorHandlingResolutionId::ReplaceStringifyingMapErr
        } else if pending > 0 {
            ErrorHandlingResolutionId::AddInfrastructureThenQuestionMark
        } else {
            ErrorHandlingResolutionId::ManualReview
        };
        types.push(
            ForeignErrorAttenuationTypeRow::builder()
                .foreign_error_type(foreign_error_type)
                .chain_preserved(preserved)
                .chain_breaks(breaks)
                .pending_infrastructure(pending)
                .total(total)
                .preservation_rate(rate)
                .primary_resolution_id(primary_resolution_id)
                .build()?,
        );
    }
    types.sort_by(|a, b| {
        b.chain_breaks()
            .cmp(&a.chain_breaks())
            .then(b.total().cmp(&a.total()))
            .then(a.foreign_error_type().cmp(b.foreign_error_type()))
    });

    WorkspaceForeignErrorAttenuationSummary::builder()
        .typed_sites(typed_sites)
        .chain_preserved(chain_preserved)
        .chain_breaks(chain_breaks)
        .pending_infrastructure(pending_infrastructure)
        .neutral(neutral)
        .preservation_rate(preservation_rate)
        .migration_backlog(chain_breaks + pending_infrastructure)
        .types(types)
        .resolutions(resolutions)
        .build()
}

#[derive(Debug, Clone, derive_new::new)]
pub struct ForeignErrorAttenuationRule {
    handling_class: ForeignErrorHandlingClass,
}

impl Rule for ForeignErrorAttenuationRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        match self.handling_class {
            ForeignErrorHandlingClass::ChainPreserved => "ERROR-HANDLING-CHAIN-PRESERVED",
            ForeignErrorHandlingClass::ChainBreak => "ERROR-HANDLING-CHAIN-BREAK",
            ForeignErrorHandlingClass::PendingInfrastructure => "ERROR-HANDLING-PENDING-INFRA",
            ForeignErrorHandlingClass::Neutral => "ERROR-HANDLING-NEUTRAL",
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "foreign_error_attenuation"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Foreign error site classified against chain-preservation probes with resolution guidance"
    }
}

#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct ForeignErrorAttenuationMarker {
    anchor: crate::objects::NodeAnchor,
}

impl Marker for ForeignErrorAttenuationMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "foreign-error-attenuation"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "foreign-error-attenuation"
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
pub struct ForeignErrorAttenuationFinding {
    rule: ForeignErrorAttenuationRule,
    #[getter(copy)]
    disposition: Disposition,
    anchor: crate::objects::NodeAnchor,
    crate_name: String,
    foreign_error_type: String,
    inference_rule_id: String,
    #[getter(copy)]
    confidence: ForeignTypeConfidence,
    #[getter(copy)]
    handling_class: ForeignErrorHandlingClass,
    #[getter(copy)]
    resolution_id: ErrorHandlingResolutionId,
    resolution: String,
    #[getter(copy)]
    kind: ErrorSiteKind,
    context: String,
    span: FileSpan,
    source_snippet: String,
    site_snippet: String,
    good_pattern: String,
    bad_pattern: String,
}

impl ForeignErrorAttenuationFinding {
    /// Start a builder for this value.
    pub fn builder() -> ForeignErrorAttenuationFindingBuilder {
        ForeignErrorAttenuationFindingBuilder::default()
    }
}

impl Finding for ForeignErrorAttenuationFinding {
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
        sink.field("handling_class", &self.handling_class.to_string());
        sink.field("resolution_id", &self.resolution_id.to_string());
        sink.field("foreign_error_type", &self.foreign_error_type);
        sink.field("inference_rule_id", &self.inference_rule_id);
        sink.field("confidence", &self.confidence.to_string());
        sink.field("context", &self.context);
        sink.field("file", &self.span.file().display().to_string());
        sink.field("line", &self.span.line().to_string());
        sink.field("site_kind", &self.kind.to_string());
        sink.field("source_snippet", &self.source_snippet);
        sink.field("site_snippet", &self.site_snippet);
        sink.field("resolution", &self.resolution);
        sink.field("good_pattern", &self.good_pattern);
        sink.field("bad_pattern", &self.bad_pattern);
        sink.snippet(&self.site_snippet);
    }
}
