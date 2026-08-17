use crate::enricher::resolve_source_path;
use crate::error::CordialResult;
use crate::hooks::Assessor;
use crate::ir::IrView;
use crate::objects::{Disposition, FileSpan, Finding, Marker};
use crate::session::SessionView;

use super::types::{DeriveFinding, DeriveRule, DeriveRuleId};

/// Converts derive-site markers into open findings.
#[derive(Debug, Default, Clone, Copy)]
pub struct DeriveAssessor;

impl DeriveAssessor {
    pub const ID: &'static str = "derive-assessor";
}

impl Assessor for DeriveAssessor {
    fn id(&self) -> &str {
        Self::ID
    }

    fn consumes(&self) -> &[&str] {
        &["derive-site"]
    }

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
            let Some(rule_value) = node.attr("derive_rule_id").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(rule_id) = DeriveRuleId::from_attr(rule_value) else {
                continue;
            };
            let struct_name = node
                .attr("struct_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let method_name = node
                .attr("method_name")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let qualified_name = node
                .attr("qualified_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let recommendation = node
                .attr("recommendation")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let evidence = node
                .attr("evidence")
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

            findings.push(Box::new(DeriveFinding {
                rule: DeriveRule::new(rule_id),
                disposition: Disposition::Open,
                anchor: crate::objects::NodeAnchor(node_id),
                crate_name: ir.crate_name().to_string(),
                struct_name,
                method_name,
                qualified_name,
                recommendation,
                span,
                evidence,
            }) as Box<dyn Finding>);
        }
        Ok(findings)
    }
}
