use crate::enricher::resolve_source_path;
use crate::error::CordialResult;
use crate::hooks::{AssessView, Assessor};
use crate::objects::{Disposition, FileSpan, Finding};

use super::types::{ProofPatternFinding, ProofPatternKind, ProofPatternRule};

use tracing::instrument;

/// Converts proof-pattern markers into open findings.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProofPatternAssessor;

impl ProofPatternAssessor {
    pub const ID: &'static str = "proof-pattern-assessor";
}

impl Assessor for ProofPatternAssessor {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn consumes(&self) -> &[&str] {
        &["proof-pattern-site"]
    }

    #[instrument(level = "trace", skip(self, view))]
    fn assess(&self, view: AssessView<'_>) -> CordialResult<Vec<Box<dyn Finding>>> {
        let markers = view.markers;
        let ir = view.ir;
        let session = view.session;

        let mut findings = Vec::new();
        for marker in markers {
            let node_id = marker.anchor().node_id();
            let Some(node) = ir.node(node_id) else {
                continue;
            };
            let Some(kind_value) = node.attr("proof_pattern_kind").and_then(|v| v.as_str())
            else {
                continue;
            };
            let Some(kind) = ProofPatternKind::from_attr(kind_value) else {
                continue;
            };
            let context = node
                .attr("context")
                .and_then(|v| v.as_str())
                .unwrap_or("<crate>")
                .to_string();
            let snippet = node
                .attr("snippet")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let line = node.attr("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let file = node
                .attr("file")
                .and_then(|v| v.as_str())
                .map(|path| resolve_source_path(session, path))
                .unwrap_or_else(|| session.project_root().to_path_buf());
            let span = FileSpan::new(file, line, 1);
            let cfg_test = node
                .attr("cfg_test")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let tracked_params = node
                .attr("tracked_params")
                .and_then(|v| v.as_str())
                .filter(|value| !value.is_empty())
                .map(|value| value.split(", ").map(str::to_string).collect())
                .unwrap_or_default();
            let recommends = node
                .attr("recommends")
                .and_then(|v| v.as_str())
                .filter(|value| !value.is_empty())
                .map(|value| value.split(", ").map(str::to_string).collect())
                .unwrap_or_default();

            findings.push(Box::new(ProofPatternFinding {
                rule: ProofPatternRule::new(kind),
                disposition: Disposition::Open,
                anchor: crate::objects::NodeAnchor(node_id),
                crate_name: ir.crate_name().to_string(),
                context,
                span,
                snippet,
                cfg_test,
                tracked_params,
                recommends,
            }) as Box<dyn Finding>);
        }
        Ok(findings)
    }
}
