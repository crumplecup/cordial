use crate::enricher::resolve_source_path;
use crate::error::CordialResult;
use crate::hooks::Assessor;
use crate::ir::IrView;
use crate::objects::{Disposition, FileSpan, Finding};
use crate::session::SessionView;

use super::types::{CfgScatterFinding, CfgScatterRule, CfgScatterRuleId};

/// Converts scattered-`cfg` group markers into open findings.
#[derive(Debug, Default, Clone, Copy)]
pub struct CfgScatterAssessor;

impl CfgScatterAssessor {
    pub const ID: &'static str = "cfg-scatter-assessor";
}

impl Assessor for CfgScatterAssessor {
    fn id(&self) -> &str {
        Self::ID
    }

    fn consumes(&self) -> &[&str] {
        &["cfg-scatter-site"]
    }

    fn assess(
        &self,
        markers: &[&dyn crate::objects::Marker],
        ir: &dyn IrView,
        session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Finding>>> {
        let mut findings = Vec::new();
        for marker in markers {
            let node_id = marker.anchor().node_id();
            let Some(node) = ir.node(node_id) else {
                continue;
            };
            let Some(predicate) = node
                .attr("cfg_scatter_predicate")
                .and_then(|v| v.as_str())
                .map(str::to_string)
            else {
                continue;
            };
            let distinct_kinds = node
                .attr("kinds")
                .and_then(|v| v.as_str())
                .map(|s| s.split('+').map(str::to_string).collect())
                .unwrap_or_default();
            let occurrence_count = node
                .attr("occurrences")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let sample_snippets = node
                .attr("sample")
                .and_then(|v| v.as_str())
                .map(|s| s.split("; ").map(str::to_string).collect())
                .unwrap_or_default();
            let file = node
                .attr("file")
                .and_then(|v| v.as_str())
                .map(|path| resolve_source_path(session, path))
                .unwrap_or_else(|| session.project_root().to_path_buf());
            let span = FileSpan::new(file, 1, 1);

            findings.push(Box::new(CfgScatterFinding {
                rule: CfgScatterRule::new(CfgScatterRuleId::Scatter001),
                disposition: Disposition::Open,
                anchor: crate::objects::NodeAnchor(node_id),
                crate_name: ir.crate_name().to_string(),
                predicate,
                span,
                distinct_kinds,
                occurrence_count,
                sample_snippets,
            }) as Box<dyn Finding>);
        }
        Ok(findings)
    }
}
