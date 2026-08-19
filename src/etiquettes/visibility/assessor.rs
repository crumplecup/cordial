use crate::error::CordialResult;
use crate::hooks::Assessor;
use crate::ir::IrView;
use crate::objects::{Disposition, FileSpan, Finding};
use crate::session::SessionView;

use super::types::{VisibilityFinding, VisibilityRule, VisibilityRuleId};

use tracing::instrument;
/// Converts visibility-path markers into open findings.
#[derive(Debug, Default, Clone, Copy)]
pub struct VisibilityAssessor;

impl VisibilityAssessor {
    pub const ID: &'static str = "visibility-assessor";
}

impl Assessor for VisibilityAssessor {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn consumes(&self) -> &[&str] {
        &["visibility-site"]
    }

    #[instrument(level = "trace", skip(self, markers, ir, session))]
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
            let Some(rule_id) = node
                .attr("visibility_rule_id")
                .and_then(|v| v.as_str())
                .and_then(VisibilityRuleId::from_attr)
            else {
                continue;
            };
            let module_path = node
                .attr("module_path")
                .and_then(|v| v.as_str())
                .unwrap_or("crate")
                .to_string();
            let name_count = node
                .attr("name_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let parent_vis = node
                .attr("parent_vis")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let declared_vis = node
                .attr("declared_vis")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let line = node.attr("line").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let file = node
                .attr("file")
                .and_then(|v| v.as_str())
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| session.project_root().to_path_buf());

            findings.push(Box::new(VisibilityFinding {
                rule: VisibilityRule::new(rule_id),
                disposition: Disposition::Open,
                anchor: crate::objects::NodeAnchor(node_id),
                crate_name: ir.crate_name().to_string(),
                module_path,
                span: FileSpan::new(file, line, 1),
                name_count,
                parent_vis,
                declared_vis,
            }) as Box<dyn Finding>);
        }
        Ok(findings)
    }
}
