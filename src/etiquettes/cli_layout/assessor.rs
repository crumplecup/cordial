use crate::enricher::resolve_source_path;
use crate::error::CordialResult;
use crate::hooks::Assessor;
use crate::ir::IrView;
use crate::objects::{Disposition, FileSpan, Finding};
use crate::session::SessionView;

use super::types::{CliLayoutFinding, CliLayoutId, CliLayoutRule};

use tracing::instrument;
/// Converts CLI-layout markers into open findings.
#[derive(Debug, Default, Clone, Copy)]
pub struct CliLayoutAssessor;

impl CliLayoutAssessor {
    pub const ID: &'static str = "cli-layout-assessor";
}

impl Assessor for CliLayoutAssessor {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn consumes(&self) -> &[&str] {
        &["cli-layout-site"]
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
                .attr("cli_layout_rule")
                .and_then(|value| value.as_str())
                .and_then(rule_from_attr)
            else {
                continue;
            };
            let context = node
                .attr("context")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            let snippet = node
                .attr("snippet")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            let line = node
                .attr("line")
                .and_then(|value| value.as_u64())
                .unwrap_or(1) as u32;
            let file = node
                .attr("file")
                .and_then(|value| value.as_str())
                .map(|path| resolve_source_path(session, path))
                .unwrap_or_else(|| session.project_root().to_path_buf());
            let span = FileSpan::new(file, line, 1);

            findings.push(Box::new(CliLayoutFinding {
                rule: CliLayoutRule::new(rule_id),
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

#[instrument(level = "debug")]
fn rule_from_attr(value: &str) -> Option<CliLayoutId> {
    match value {
        "CLI-ISLAND-001" => Some(CliLayoutId::Island001),
        "CLI-ACT-001" => Some(CliLayoutId::Act001),
        "CLI-MAIN-001" => Some(CliLayoutId::Main001),
        _ => None,
    }
}
