use crate::enricher::resolve_source_path;
use crate::error::CordialResult;
use crate::etiquettes::error_sites::{ErrorOriginClass, ErrorSiteKind, ForeignTypeConfidence};
use crate::hooks::Assessor;
use crate::ir::IrView;
use crate::objects::{Disposition, FileSpan, Finding, Marker};
use crate::session::SessionView;

use super::types::{ForeignErrorRecordKind, ForeignErrorTypeFinding, ForeignErrorTypeRule};

use tracing::instrument;
#[derive(Debug, Default, Clone, Copy)]
pub struct ForeignErrorTypeAssessor;

impl ForeignErrorTypeAssessor {
    pub const ID: &'static str = "foreign-error-type-assessor";
}

impl Assessor for ForeignErrorTypeAssessor {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn consumes(&self) -> &[&str] {
        &["foreign-error-type"]
    }

    #[instrument(level = "trace", skip(self, markers, ir, session))]
    fn assess(
        &self,
        markers: &[&dyn Marker],
        ir: &dyn IrView,
        session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Finding>>> {
        let crate_name = ir.crate_name().to_string();
        let mut findings = Vec::new();
        for marker in markers {
            let node_id = marker.anchor().node_id();
            let Some(node) = ir.node(node_id) else {
                continue;
            };
            let Some(kind_value) = node
                .attr("foreign_error_record_kind")
                .and_then(|value| value.as_str())
            else {
                continue;
            };
            let Some(record_kind) = ForeignErrorRecordKind::from_attr(kind_value) else {
                continue;
            };
            let context = node
                .attr("context")
                .and_then(|value| value.as_str())
                .unwrap_or("<crate>")
                .to_string();
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
                .attr("error_site_kind")
                .and_then(|value| value.as_str())
                .and_then(ErrorSiteKind::from_attr)
                .or_else(|| {
                    node.attr("site_kind")
                        .and_then(|value| value.as_str())
                        .and_then(ErrorSiteKind::from_attr)
                })
                .unwrap_or(ErrorSiteKind::QuestionMark);

            match record_kind {
                ForeignErrorRecordKind::Typed => {
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
                    let chain_break = node
                        .attr("chain_break")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false)
                        && site_kind.map_err_is_chain_break(false);

                    findings.push(Box::new(ForeignErrorTypeFinding {
                        rule: ForeignErrorTypeRule::new(inference_rule_id.clone()),
                        disposition: Disposition::Open,
                        anchor: crate::objects::NodeAnchor(node_id),
                        record_kind,
                        crate_name: crate_name.clone(),
                        foreign_error_type,
                        inference_rule_id,
                        confidence,
                        chain_break,
                        kind: site_kind,
                        context,
                        span,
                        source_snippet,
                        site_snippet,
                        origin_class: ErrorOriginClass::Other,
                        origin_detail: String::new(),
                        rationale: String::new(),
                    }) as Box<dyn Finding>);
                }
                ForeignErrorRecordKind::Candidate => {
                    let origin_class = node
                        .attr("origin_class")
                        .and_then(|value| value.as_str())
                        .map(parse_origin_class)
                        .unwrap_or(ErrorOriginClass::Edge);
                    let origin_detail = node
                        .attr("origin_detail")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string();
                    let rationale = node
                        .attr("rationale")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string();

                    findings.push(Box::new(ForeignErrorTypeFinding {
                        rule: ForeignErrorTypeRule::new("FOREIGN-ERROR-CANDIDATE"),
                        disposition: Disposition::Open,
                        anchor: crate::objects::NodeAnchor(node_id),
                        record_kind: ForeignErrorRecordKind::Candidate,
                        crate_name: crate_name.clone(),
                        foreign_error_type: String::new(),
                        inference_rule_id: String::new(),
                        confidence: ForeignTypeConfidence::High,
                        chain_break: false,
                        kind: site_kind,
                        context,
                        span,
                        source_snippet,
                        site_snippet,
                        origin_class,
                        origin_detail,
                        rationale,
                    }) as Box<dyn Finding>);
                }
            }
        }
        Ok(findings)
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

#[instrument(level = "debug")]
fn parse_origin_class(value: &str) -> ErrorOriginClass {
    if value.contains("OTHER") {
        ErrorOriginClass::Other
    } else if value.contains("EDGE") {
        ErrorOriginClass::Edge
    } else {
        ErrorOriginClass::Internal
    }
}
