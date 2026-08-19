use crate::enricher::resolve_shadow_entries;
use crate::error::CordialResult;
use crate::hooks::Probe;
use crate::ir::{EdgeKind, IrView, Query};
use crate::objects::Marker;
use crate::session::SessionView;

use super::types::MissingMirrorMarker;

use tracing::instrument;
#[derive(Debug, Default, Clone, Copy)]
struct ShadowTargetQuery;

impl Query for ShadowTargetQuery {
    #[instrument(level = "trace", skip(self))]
    fn node_kinds(&self) -> &[crate::ir::NodeKind] {
        &[]
    }

    #[instrument(level = "trace", skip(self))]
    fn edge_kinds(&self) -> &[EdgeKind] {
        &[]
    }

    #[instrument(level = "trace", skip(self, node))]
    fn matches_node(&self, node: &dyn crate::ir::NodeView) -> bool {
        node.attr("shadow_path").is_some()
    }
}

static SHADOW_TARGET_QUERY: ShadowTargetQuery = ShadowTargetQuery;

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

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &SHADOW_TARGET_QUERY
    }

    #[instrument(level = "trace", skip(self, ir, session))]
    fn probe(
        &self,
        ir: &dyn IrView,
        session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Marker>>> {
        let entries = resolve_shadow_entries(session, ir)?;
        let mut markers = Vec::new();

        for entry in entries {
            let Some(target) = ir.node_by_path(&entry.target) else {
                continue;
            };
            let mirrors = ir.children(target, EdgeKind::Mirrors);
            if mirrors.iter().any(|shadow| {
                ir.node(*shadow).is_some_and(|node| {
                    node.attr("qualified_path").and_then(|v| v.as_str())
                        == Some(entry.shadow.as_str())
                })
            }) {
                continue;
            }
            markers.push(Box::new(MissingMirrorMarker {
                anchor: crate::objects::NodeAnchor(target),
            }) as Box<dyn Marker>);
        }

        let _ = SHADOW_TARGET_QUERY;
        Ok(markers)
    }
}
