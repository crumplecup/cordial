use crate::enricher::resolve_shadow_entries;
use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::EdgeKind;
use crate::objects::Marker;

use super::types::MissingMirrorMarker;

use tracing::instrument;

#[derive(Debug, Default, Clone, Copy)]
pub struct MissingShadowMirrorProbe;

impl MissingShadowMirrorProbe {
    pub const ID: &'static str = "missing-shadow-mirror";
}

impl Probe for MissingShadowMirrorProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let (ir, _, session) = view.into_parts();

        let entries = resolve_shadow_entries(session, ir)?;
        let mut markers = Vec::new();

        for entry in entries {
            let Some(target) = ir.node_by_path(entry.target()) else {
                continue;
            };
            let mirrors = ir.children(target, EdgeKind::Mirrors);
            if mirrors.iter().any(|shadow| {
                ir.node(*shadow).is_some_and(|node| {
                    node.attr("qualified_path").and_then(|v| v.as_str())
                        == Some(entry.shadow().as_str())
                })
            }) {
                continue;
            }
            markers.push(
                Box::new(MissingMirrorMarker::new(crate::objects::NodeAnchor::new(
                    target,
                ))) as Box<dyn Marker>,
            );
        }

        Ok(markers)
    }
}
