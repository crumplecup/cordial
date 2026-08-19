use crate::enricher::resolve_source_path;
use crate::error::CordialResult;
use crate::hooks::{AssessView, Assessor};
use crate::objects::{Disposition, FileSpan, Finding};

use super::types::{
    InternalErrorChainFinding, InternalErrorChainRule, InternalErrorComplianceId,
    InternalErrorNodeClass, InternalErrorRecordKind, InternalErrorTypeProbeId,
};

use tracing::instrument;
/// Converts internal error-chain markers into findings.
#[derive(Debug, Default, Clone, Copy)]
pub struct InternalErrorChainAssessor;

impl InternalErrorChainAssessor {
    pub const ID: &'static str = "internal-error-chain-assessor";
}

impl Assessor for InternalErrorChainAssessor {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn consumes(&self) -> &[&str] {
        &["internal-error-chain"]
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
            let Some(kind_value) = node
                .attr("internal_error_record_kind")
                .and_then(|value| value.as_str())
            else {
                continue;
            };
            let Some(record_kind) = InternalErrorRecordKind::from_attr(kind_value) else {
                continue;
            };
            let snippet = node
                .attr("snippet")
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

            match record_kind {
                InternalErrorRecordKind::TypeGraph => {
                    let Some(probe_value) = node.attr("probe_id").and_then(|value| value.as_str())
                    else {
                        continue;
                    };
                    let Some(probe_id) = InternalErrorTypeProbeId::from_attr(probe_value) else {
                        continue;
                    };
                    let type_path = node
                        .attr("type_path")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string();
                    let node_class = node
                        .attr("node_class")
                        .and_then(|value| value.as_str())
                        .and_then(InternalErrorNodeClass::from_attr);
                    let source_target = node
                        .attr("source_target")
                        .and_then(|value| value.as_str())
                        .filter(|value| !value.is_empty())
                        .map(str::to_string);
                    let reaches_foreign = node
                        .attr("reaches_foreign")
                        .and_then(|value| value.as_bool());
                    let chain_depth = node
                        .attr("chain_depth")
                        .and_then(|value| value.as_u64())
                        .map(|value| value as u32);

                    findings.push(Box::new(InternalErrorChainFinding {
                        rule: InternalErrorChainRule::from_probe(probe_id),
                        record_kind,
                        disposition: Disposition::Open,
                        anchor: crate::objects::NodeAnchor(node_id),
                        crate_name: crate_name.clone(),
                        context: type_path.clone(),
                        span,
                        snippet,
                        type_path: Some(type_path),
                        node_class,
                        source_target,
                        reaches_foreign,
                        chain_depth,
                        foreign_error_type: None,
                        internal_constructor: None,
                    }) as Box<dyn Finding>);
                }
                InternalErrorRecordKind::Compliance => {
                    let Some(rule_value) = node.attr("rule_id").and_then(|value| value.as_str())
                    else {
                        continue;
                    };
                    let Some(compliance_id) = InternalErrorComplianceId::from_attr(rule_value)
                    else {
                        continue;
                    };
                    let context = node
                        .attr("context")
                        .and_then(|value| value.as_str())
                        .unwrap_or("<crate>")
                        .to_string();
                    let foreign_error_type = node
                        .attr("foreign_error_type")
                        .and_then(|value| value.as_str())
                        .filter(|value| !value.is_empty())
                        .map(str::to_string);
                    let internal_constructor = node
                        .attr("internal_constructor")
                        .and_then(|value| value.as_str())
                        .filter(|value| !value.is_empty())
                        .map(str::to_string);

                    findings.push(Box::new(InternalErrorChainFinding {
                        rule: InternalErrorChainRule::from_compliance(compliance_id),
                        record_kind,
                        disposition: Disposition::Open,
                        anchor: crate::objects::NodeAnchor(node_id),
                        crate_name: crate_name.clone(),
                        context,
                        span,
                        snippet,
                        type_path: None,
                        node_class: None,
                        source_target: None,
                        reaches_foreign: None,
                        chain_depth: None,
                        foreign_error_type,
                        internal_constructor,
                    }) as Box<dyn Finding>);
                }
            }
        }
        Ok(findings)
    }
}
