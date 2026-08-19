use crate::enricher::resolve_source_path;
use crate::error::CordialResult;
use crate::hooks::Assessor;
use crate::ir::IrView;
use crate::objects::{Disposition, FileSpan, Finding, Marker};
use crate::session::SessionView;

use super::types::{AntipatternFinding, AntipatternRule, AntipatternRuleId};

use tracing::instrument;
/// Converts antipattern-site markers into open findings.
#[derive(Debug, Default, Clone, Copy)]
pub struct AntipatternAssessor;

impl AntipatternAssessor {
    pub const ID: &'static str = "antipattern-assessor";
}

impl Assessor for AntipatternAssessor {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn consumes(&self) -> &[&str] {
        &["antipattern-site"]
    }

    #[instrument(level = "trace", skip(self, markers, ir, session))]
    fn assess(
        &self,
        markers: &[&dyn Marker],
        ir: &dyn IrView,
        session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Finding>>> {
        let mut findings = Vec::new();
        for marker in markers {
            let node_id = marker.anchor().node_id();
            let Some(node) = ir.node(node_id) else {
                continue;
            };
            let Some(rule_value) = node.attr("antipattern_rule_id").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(rule_id) = AntipatternRuleId::from_attr(rule_value) else {
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

            findings.push(Box::new(AntipatternFinding {
                rule: AntipatternRule::new(rule_id),
                disposition: Disposition::Open,
                anchor: crate::objects::NodeAnchor(node_id),
                crate_name: ir.crate_name().to_string(),
                context,
                span,
                snippet,
            }) as Box<dyn Finding>);
        }
        Ok(findings)
    }
}
