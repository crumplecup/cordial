use crate::enricher::resolve_source_path;
use crate::error::CordialResult;
use crate::hooks::Assessor;
use crate::ir::IrView;
use crate::objects::{Disposition, FileSpan, Finding, Marker};
use crate::session::SessionView;

use super::types::{FunctionKind, TracingFinding, TracingRule, TracingRuleKind, VisibilityLabel};

/// Converts missing-instrument markers into open findings.
#[derive(Debug, Default, Clone, Copy)]
pub struct TracingAssessor;

impl TracingAssessor {
    pub const ID: &'static str = "tracing-assessor";
}

impl Assessor for TracingAssessor {
    fn id(&self) -> &str {
        Self::ID
    }

    fn consumes(&self) -> &[&str] {
        &["missing-instrument"]
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
            let qualified_name = node
                .attr("qualified_path")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            let kind = node
                .attr("function_kind")
                .and_then(|v| v.as_str())
                .map(parse_function_kind)
                .unwrap_or(FunctionKind::Free);
            let visibility = node
                .attr("visibility")
                .and_then(|v| v.as_str())
                .map(parse_visibility)
                .unwrap_or(VisibilityLabel::Private);
            if !should_report(&visibility) {
                continue;
            }
            let line = node.attr("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let file = node
                .attr("file")
                .and_then(|v| v.as_str())
                .map(|path| resolve_source_path(session, path))
                .unwrap_or_else(|| session.project_root().to_path_buf());
            let span = FileSpan::new(file, line, 1);

            findings.push(Box::new(TracingFinding {
                rule: TracingRule::new(TracingRuleKind::MissingInstrument),
                disposition: Disposition::Open,
                anchor: crate::objects::NodeAnchor(node_id),
                crate_name: ir.crate_name().to_string(),
                qualified_name,
                kind,
                visibility,
                span,
            }) as Box<dyn Finding>);
        }
        Ok(findings)
    }
}

fn should_report(visibility: &VisibilityLabel) -> bool {
    matches!(
        visibility,
        VisibilityLabel::Public | VisibilityLabel::PubCrate
    )
}

fn parse_function_kind(value: &str) -> FunctionKind {
    match value {
        "inherent" => FunctionKind::InherentMethod,
        "trait_impl" => FunctionKind::TraitImplMethod,
        _ => FunctionKind::Free,
    }
}

fn parse_visibility(value: &str) -> VisibilityLabel {
    match value {
        "pub" => VisibilityLabel::Public,
        "pub(crate)" => VisibilityLabel::PubCrate,
        "pub(super)" => VisibilityLabel::PubSuper,
        other if other.starts_with("pub(") => VisibilityLabel::PubInPath(
            other
                .trim_start_matches("pub(")
                .trim_end_matches(')')
                .to_string(),
        ),
        _ => VisibilityLabel::Private,
    }
}
