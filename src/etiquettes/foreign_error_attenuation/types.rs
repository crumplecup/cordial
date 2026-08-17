use std::collections::BTreeMap;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use crate::etiquettes::error_sites::{ErrorSiteKind, ForeignTypeConfidence};
use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

/// How a typed foreign error site aligns with chain-preservation probes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ForeignErrorHandlingClass {
    ChainPreserved,
    ChainBreak,
    PendingInfrastructure,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignErrorAttenuationRecord {
    pub crate_name: String,
    pub foreign_error_type: String,
    pub inference_rule_id: String,
    pub confidence: ForeignTypeConfidence,
    pub handling_class: ForeignErrorHandlingClass,
    pub resolution_id: ErrorHandlingResolutionId,
    pub resolution: String,
    pub kind: ErrorSiteKind,
    pub context: String,
    pub file: PathBuf,
    pub line: u32,
    pub source_snippet: String,
    pub site_snippet: String,
    pub good_pattern: String,
    pub bad_pattern: String,
}

/// Attenuation report for one crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignErrorAttenuationReport {
    pub crate_name: String,
    pub findings: Vec<ForeignErrorAttenuationRecord>,
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
    pub fn handling_counts(&self) -> ForeignErrorHandlingCounts {
        let mut counts = ForeignErrorHandlingCounts::default();
        for finding in &self.findings {
            match finding.handling_class {
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
#[derive(Debug, Clone, PartialEq)]
pub struct ForeignErrorAttenuationTypeRow {
    pub foreign_error_type: String,
    pub chain_preserved: usize,
    pub chain_breaks: usize,
    pub pending_infrastructure: usize,
    pub total: usize,
    pub preservation_rate: Option<f64>,
    pub primary_resolution_id: ErrorHandlingResolutionId,
}

/// Workspace attenuation metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceForeignErrorAttenuationSummary {
    pub typed_sites: usize,
    pub chain_preserved: usize,
    pub chain_breaks: usize,
    pub pending_infrastructure: usize,
    pub neutral: usize,
    pub preservation_rate: Option<f64>,
    pub migration_backlog: usize,
    pub types: Vec<ForeignErrorAttenuationTypeRow>,
    pub resolutions: BTreeMap<String, usize>,
}

pub fn build_workspace_foreign_error_attenuation_summary(
    reports: &[ForeignErrorAttenuationReport],
) -> WorkspaceForeignErrorAttenuationSummary {
    let mut chain_preserved = 0usize;
    let mut chain_breaks = 0usize;
    let mut pending_infrastructure = 0usize;
    let mut neutral = 0usize;
    let mut by_type: BTreeMap<String, (usize, usize, usize, usize)> = BTreeMap::new();
    let mut resolutions: BTreeMap<String, usize> = BTreeMap::new();

    for report in reports {
        for finding in &report.findings {
            chain_preserved +=
                usize::from(finding.handling_class == ForeignErrorHandlingClass::ChainPreserved);
            chain_breaks +=
                usize::from(finding.handling_class == ForeignErrorHandlingClass::ChainBreak);
            pending_infrastructure += usize::from(
                finding.handling_class == ForeignErrorHandlingClass::PendingInfrastructure,
            );
            neutral += usize::from(finding.handling_class == ForeignErrorHandlingClass::Neutral);
            *resolutions
                .entry(finding.resolution_id.to_string())
                .or_default() += 1;

            let entry = by_type
                .entry(finding.foreign_error_type.clone())
                .or_default();
            entry.3 += 1;
            match finding.handling_class {
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

    let mut types: Vec<ForeignErrorAttenuationTypeRow> = by_type
        .into_iter()
        .map(
            |(foreign_error_type, (preserved, breaks, pending, total))| {
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
                ForeignErrorAttenuationTypeRow {
                    foreign_error_type,
                    chain_preserved: preserved,
                    chain_breaks: breaks,
                    pending_infrastructure: pending,
                    total,
                    preservation_rate: rate,
                    primary_resolution_id,
                }
            },
        )
        .collect();
    types.sort_by(|a, b| {
        b.chain_breaks
            .cmp(&a.chain_breaks)
            .then(b.total.cmp(&a.total))
            .then(a.foreign_error_type.cmp(&b.foreign_error_type))
    });

    WorkspaceForeignErrorAttenuationSummary {
        typed_sites,
        chain_preserved,
        chain_breaks,
        pending_infrastructure,
        neutral,
        preservation_rate,
        migration_backlog: chain_breaks + pending_infrastructure,
        types,
        resolutions,
    }
}

#[derive(Debug, Clone)]
pub struct ForeignErrorAttenuationRule {
    pub handling_class: ForeignErrorHandlingClass,
}

impl ForeignErrorAttenuationRule {
    pub fn new(handling_class: ForeignErrorHandlingClass) -> Self {
        Self { handling_class }
    }
}

impl Rule for ForeignErrorAttenuationRule {
    fn id(&self) -> &str {
        match self.handling_class {
            ForeignErrorHandlingClass::ChainPreserved => "ERROR-HANDLING-CHAIN-PRESERVED",
            ForeignErrorHandlingClass::ChainBreak => "ERROR-HANDLING-CHAIN-BREAK",
            ForeignErrorHandlingClass::PendingInfrastructure => "ERROR-HANDLING-PENDING-INFRA",
            ForeignErrorHandlingClass::Neutral => "ERROR-HANDLING-NEUTRAL",
        }
    }

    fn category(&self) -> &str {
        "foreign_error_attenuation"
    }

    fn description(&self) -> &str {
        "Foreign error site classified against chain-preservation probes with resolution guidance"
    }
}

#[derive(Debug, Clone)]
pub struct ForeignErrorAttenuationMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for ForeignErrorAttenuationMarker {
    fn probe(&self) -> &str {
        "foreign-error-attenuation"
    }

    fn label(&self) -> &str {
        "foreign-error-attenuation"
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    fn span(&self) -> Option<&dyn SourceSpan> {
        None
    }
}

#[derive(Debug, Clone)]
pub struct ForeignErrorAttenuationFinding {
    pub rule: ForeignErrorAttenuationRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub foreign_error_type: String,
    pub inference_rule_id: String,
    pub confidence: ForeignTypeConfidence,
    pub handling_class: ForeignErrorHandlingClass,
    pub resolution_id: ErrorHandlingResolutionId,
    pub resolution: String,
    pub kind: ErrorSiteKind,
    pub context: String,
    pub span: FileSpan,
    pub source_snippet: String,
    pub site_snippet: String,
    pub good_pattern: String,
    pub bad_pattern: String,
}

impl Finding for ForeignErrorAttenuationFinding {
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
        sink.field("handling_class", &self.handling_class.to_string());
        sink.field("resolution_id", &self.resolution_id.to_string());
        sink.field("foreign_error_type", &self.foreign_error_type);
        sink.field("inference_rule_id", &self.inference_rule_id);
        sink.field("confidence", &self.confidence.to_string());
        sink.field("context", &self.context);
        sink.field("file", &self.span.file.display().to_string());
        sink.field("line", &self.span.line.to_string());
        sink.field("site_kind", &self.kind.to_string());
        sink.field("source_snippet", &self.source_snippet);
        sink.field("site_snippet", &self.site_snippet);
        sink.field("resolution", &self.resolution);
        sink.field("good_pattern", &self.good_pattern);
        sink.field("bad_pattern", &self.bad_pattern);
        sink.snippet(&self.site_snippet);
    }
}
