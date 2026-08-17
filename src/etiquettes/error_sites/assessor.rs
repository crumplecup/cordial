use crate::enricher::resolve_source_path;
use crate::error::CordialResult;
use crate::hooks::Assessor;
use crate::ir::IrView;
use crate::objects::{Disposition, FileSpan, Finding, Marker};
use crate::session::SessionView;

use super::types::{ErrorOriginClass, ErrorSiteFinding, ErrorSiteKind, ErrorSiteRule};

/// Converts error-site markers into partitioned findings.
#[derive(Debug, Default, Clone, Copy)]
pub struct ErrorSiteAssessor;

impl ErrorSiteAssessor {
    pub const ID: &'static str = "error-site-assessor";
}

impl Assessor for ErrorSiteAssessor {
    fn id(&self) -> &str {
        Self::ID
    }

    fn consumes(&self) -> &[&str] {
        &["error-site"]
    }

    fn assess(
        &self,
        markers: &[&dyn Marker],
        ir: &dyn IrView,
        session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Finding>>> {
        let crate_name = ir.crate_name().to_string();
        let mut findings = Vec::new();
        for marker in markers {
            let node_id = marker.anchor().node_id();
            let Some(node) = ir.node(node_id) else {
                continue;
            };
            let Some(kind_value) = node.attr("error_site_kind").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(kind) = ErrorSiteKind::from_attr(kind_value) else {
                continue;
            };
            let context = node
                .attr("context")
                .and_then(|v| v.as_str())
                .unwrap_or("<crate>")
                .to_string();
            let source_snippet = node
                .attr("source_snippet")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let site_snippet = node
                .attr("site_snippet")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let line = node.attr("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let file = node
                .attr("file")
                .and_then(|v| v.as_str())
                .map(|path| resolve_source_path(session, path))
                .unwrap_or_else(|| session.project_root().to_path_buf());
            let span = FileSpan::new(file.clone(), line, 1);

            let origin_class = node
                .attr("origin_class")
                .and_then(|value| value.as_str())
                .map(parse_origin_class)
                .unwrap_or(ErrorOriginClass::Edge);
            let origin_detail = node
                .attr("origin_detail")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            let rationale = node
                .attr("rationale")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();

            findings.push(Box::new(ErrorSiteFinding {
                rule: ErrorSiteRule::new(kind),
                disposition: Disposition::Open,
                anchor: crate::objects::NodeAnchor(node_id),
                crate_name: crate_name.clone(),
                kind,
                context,
                span,
                source_snippet,
                site_snippet,
                origin_class,
                origin_detail,
                rationale,
            }) as Box<dyn Finding>);
        }
        Ok(findings)
    }
}

fn parse_origin_class(value: &str) -> ErrorOriginClass {
    if value.contains("OTHER") {
        ErrorOriginClass::Other
    } else if value.contains("EDGE") {
        ErrorOriginClass::Edge
    } else {
        ErrorOriginClass::Internal
    }
}
