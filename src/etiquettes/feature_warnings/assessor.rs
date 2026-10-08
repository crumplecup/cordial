use crate::enricher::resolve_source_path;
use crate::error::CordialResult;
use crate::hooks::{AssessView, Assessor};
use crate::objects::{Disposition, FileSpan, Finding};

use super::types::{FeatureWarningFinding, FeatureWarningRule, FeatureWarningRuleId};

use tracing::instrument;

/// Converts feature-warning markers into open findings.
#[derive(Debug, Default, Clone, Copy)]
pub struct FeatureWarningAssessor;

impl FeatureWarningAssessor {
    /// Stable identifier for `FeatureWarningAssessor`.
    pub const ID: &'static str = "feature-warning-assessor";
}

impl Assessor for FeatureWarningAssessor {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn consumes(&self) -> Vec<&str> {
        vec!["feature-warning-site"]
    }

    #[instrument(level = "trace", skip(self, view))]
    fn assess(&self, view: AssessView<'_>) -> CordialResult<Vec<Box<dyn Finding>>> {
        let markers = view.markers();
        let (ir, _, session) = view.into_parts();

        let mut findings = Vec::new();
        for marker in markers {
            let node_id = marker.anchor().node_id();
            let Some(node) = ir.node(node_id) else {
                continue;
            };
            let attr = |name: &str| {
                node.attr(name)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            let Some(rule_id) = FeatureWarningRuleId::from_attr(&attr("feature_warning_rule_id"))
            else {
                continue;
            };
            let line = node.attr("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let file = node
                .attr("file")
                .and_then(|v| v.as_str())
                .map(|path| resolve_source_path(session, path))
                .unwrap_or_else(|| session.project_root().to_path_buf());

            findings.push(Box::new(
                FeatureWarningFinding::builder()
                    .rule(FeatureWarningRule::new(rule_id))
                    .disposition(Disposition::Open)
                    .anchor(crate::objects::NodeAnchor::new(node_id))
                    .crate_name(ir.crate_name().to_string())
                    .lint(attr("lint"))
                    .span(FileSpan::new(file, line, 1))
                    .message(attr("message"))
                    .gate(attr("gate"))
                    .wide(node.attr("wide").and_then(|v| v.as_bool()).unwrap_or(false))
                    .advice(attr("advice"))
                    .triggering(attr("triggering"))
                    .build()?,
            ) as Box<dyn Finding>);
        }
        Ok(findings)
    }
}
