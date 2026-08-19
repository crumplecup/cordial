use crate::error::CordialResult;
use crate::hooks::Assessor;
use crate::ir::IrView;
use crate::objects::{Disposition, Finding, Marker};
use crate::session::SessionView;

use super::types::{MissingMirrorFinding, ShadowRule};

use tracing::instrument;
#[derive(Debug, Default, Clone, Copy)]
pub struct ShadowAssessor;

impl ShadowAssessor {
    pub const ID: &'static str = "shadow-assessor";
}

impl Assessor for ShadowAssessor {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn consumes(&self) -> &[&str] {
        &["missing-shadow-mirror"]
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
            let target_path = node
                .attr("qualified_path")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            let shadow_path = node
                .attr("shadow_path")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            findings.push(Box::new(MissingMirrorFinding {
                rule: ShadowRule,
                disposition: Disposition::Open,
                anchor: crate::objects::NodeAnchor(node_id),
                crate_name: ir.crate_name().to_string(),
                target_path,
                shadow_path,
            }) as Box<dyn Finding>);
        }
        Ok(findings)
    }
}
