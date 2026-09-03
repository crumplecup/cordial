use crate::enricher::resolve_source_path;
use crate::error::CordialResult;
use crate::hooks::{AssessView, Assessor};
use crate::objects::{Disposition, FileSpan, Finding};

use super::types::{PRINT_SITE_LABEL, PrintFinding, PrintRule, PrintRuleId};

use tracing::instrument;

/// Converts leftover-print markers into open findings.
#[derive(Debug, Default, Clone, Copy)]
pub struct PrintAssessor;

impl PrintAssessor {
    pub const ID: &'static str = "tracing-print-assessor";
}

impl Assessor for PrintAssessor {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn consumes(&self) -> &[&str] {
        &[PRINT_SITE_LABEL]
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
            let Some(rule_value) = node.attr("print_rule_id").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(rule_id) = PrintRuleId::from_attr(rule_value) else {
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

            findings.push(Box::new(
                PrintFinding::builder()
                    .rule(PrintRule::new(rule_id))
                    .disposition(Disposition::Open)
                    .anchor(crate::objects::NodeAnchor(node_id))
                    .crate_name(ir.crate_name().to_string())
                    .context(context)
                    .span(span)
                    .snippet(snippet)
                    .build()?,
            ) as Box<dyn Finding>);
        }
        Ok(findings)
    }
}
