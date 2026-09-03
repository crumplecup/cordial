use crate::enricher::resolve_source_path;
use crate::error::CordialResult;
use crate::etiquettes::error_sites::{ErrorSiteKind, ForeignTypeConfidence};
use crate::hooks::{AssessView, Assessor};
use crate::objects::{Disposition, FileSpan, Finding};

use super::types::{
    ErrorHandlingResolutionId, ForeignErrorAttenuationFinding, ForeignErrorAttenuationRule,
    ForeignErrorHandlingClass,
};

use tracing::instrument;
#[derive(Debug, Default, Clone, Copy)]
pub struct ForeignErrorAttenuationAssessor;

impl ForeignErrorAttenuationAssessor {
    pub const ID: &'static str = "foreign-error-attenuation-assessor";
}

impl Assessor for ForeignErrorAttenuationAssessor {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn consumes(&self) -> &[&str] {
        &["foreign-error-attenuation"]
    }

    #[instrument(level = "trace", skip(self, view))]
    fn assess(&self, view: AssessView<'_>) -> CordialResult<Vec<Box<dyn Finding>>> {
        let markers = view.markers;
        let ir = view.ir;
        let session = view.session;

        let crate_name = ir.crate_name().to_string();
        let mut findings = Vec::new();
        for marker in markers {
            let node_id = marker.anchor().node_id();
            let Some(node) = ir.node(node_id) else {
                continue;
            };
            let handling_class = node
                .attr("handling_class")
                .and_then(|value| value.as_str())
                .map(parse_handling_class)
                .unwrap_or(ForeignErrorHandlingClass::Neutral);
            let resolution_id = node
                .attr("resolution_id")
                .and_then(|value| value.as_str())
                .map(parse_resolution_id)
                .unwrap_or(ErrorHandlingResolutionId::ManualReview);
            let context = node
                .attr("context")
                .and_then(|value| value.as_str())
                .unwrap_or("<crate>")
                .to_string();
            let foreign_error_type = node
                .attr("foreign_error_type")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            let inference_rule_id = node
                .attr("inference_rule_id")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            let confidence = node
                .attr("confidence")
                .and_then(|value| value.as_str())
                .map(parse_confidence)
                .unwrap_or(ForeignTypeConfidence::High);
            let source_snippet = node
                .attr("source_snippet")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            let site_snippet = node
                .attr("site_snippet")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            let resolution = node
                .attr("resolution")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            let good_pattern = node
                .attr("good_pattern")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            let bad_pattern = node
                .attr("bad_pattern")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            let line = node
                .attr("line")
                .and_then(|value| value.as_u64())
                .unwrap_or(0) as u32;
            let file = node
                .attr("file")
                .and_then(|value| value.as_str())
                .map(|path| resolve_source_path(session, path))
                .unwrap_or_else(|| session.project_root().to_path_buf());
            let span = FileSpan::new(file, line, 1);
            let site_kind = node
                .attr("site_kind")
                .and_then(|value| value.as_str())
                .and_then(ErrorSiteKind::from_attr)
                .unwrap_or(ErrorSiteKind::QuestionMark);

            findings.push(Box::new(
                ForeignErrorAttenuationFinding::builder()
                    .rule(ForeignErrorAttenuationRule::new(handling_class))
                    .disposition(Disposition::Open)
                    .anchor(crate::objects::NodeAnchor(node_id))
                    .crate_name(crate_name.clone())
                    .foreign_error_type(foreign_error_type)
                    .inference_rule_id(inference_rule_id)
                    .confidence(confidence)
                    .handling_class(handling_class)
                    .resolution_id(resolution_id)
                    .resolution(resolution)
                    .kind(site_kind)
                    .context(context)
                    .span(span)
                    .source_snippet(source_snippet)
                    .site_snippet(site_snippet)
                    .good_pattern(good_pattern)
                    .bad_pattern(bad_pattern)
                    .build()?,
            ) as Box<dyn Finding>);
        }
        Ok(findings)
    }
}

#[instrument(level = "debug")]
fn parse_handling_class(value: &str) -> ForeignErrorHandlingClass {
    if value.contains("CHAIN-PRESERVED") {
        ForeignErrorHandlingClass::ChainPreserved
    } else if value.contains("CHAIN-BREAK") {
        ForeignErrorHandlingClass::ChainBreak
    } else if value.contains("PENDING-INFRA") {
        ForeignErrorHandlingClass::PendingInfrastructure
    } else {
        ForeignErrorHandlingClass::Neutral
    }
}

#[instrument(level = "debug")]
fn parse_resolution_id(value: &str) -> ErrorHandlingResolutionId {
    if value.contains("MAINTAIN-EXEMPLAR") {
        ErrorHandlingResolutionId::MaintainExemplar
    } else if value.contains("REPLACE-STRINGIFY-MAP-ERR") {
        ErrorHandlingResolutionId::ReplaceStringifyingMapErr
    } else if value.contains("ADD-INFRA-THEN-QUESTION-MARK") {
        ErrorHandlingResolutionId::AddInfrastructureThenQuestionMark
    } else {
        ErrorHandlingResolutionId::ManualReview
    }
}

#[instrument(level = "debug")]
fn parse_confidence(value: &str) -> ForeignTypeConfidence {
    if value.contains("MEDIUM") {
        ForeignTypeConfidence::Medium
    } else {
        ForeignTypeConfidence::High
    }
}
