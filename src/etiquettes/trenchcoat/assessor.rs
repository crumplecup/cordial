use crate::error::CordialResult;
use crate::hooks::Assessor;
use crate::ir::IrView;
use crate::objects::{Disposition, Finding, Marker};
use crate::session::SessionView;

use super::types::{TrenchcoatRule, UnwrappedFinding};

use tracing::instrument;
#[derive(Debug, Default, Clone, Copy)]
pub struct TrenchcoatAssessor;

impl TrenchcoatAssessor {
    pub const ID: &'static str = "trenchcoat-assessor";
}

impl Assessor for TrenchcoatAssessor {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn consumes(&self) -> &[&str] {
        &["unwrapped-foreign"]
    }

    #[instrument(level = "trace", skip(self, markers, ir, _session))]
    fn assess(
        &self,
        markers: &[&dyn Marker],
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Finding>>> {
        let mut findings = Vec::new();
        for marker in markers {
            let node_id = marker.anchor().node_id();
            let Some(node) = ir.node(node_id) else {
                continue;
            };
            let type_path = node
                .attr("qualified_path")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            findings.push(Box::new(UnwrappedFinding {
                rule: TrenchcoatRule,
                disposition: Disposition::Open,
                anchor: crate::objects::NodeAnchor(node_id),
                crate_name: ir.crate_name().to_string(),
                type_path,
            }) as Box<dyn Finding>);
        }
        Ok(findings)
    }
}
