use crate::error::CordialResult;
use crate::hooks::Probe;
use crate::ir::{IrView, NodeKind, Query};
use crate::objects::Marker;
use crate::session::SessionView;

use super::types::VisibilityMarker;

use tracing::instrument;
/// Matches visibility-path nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct VisibilitySitesQuery;

impl Query for VisibilitySitesQuery {
    #[instrument(level = "trace", skip(self))]
    fn node_kinds(&self) -> &[NodeKind] {
        &[NodeKind::Expr]
    }

    #[instrument(level = "trace", skip(self))]
    fn edge_kinds(&self) -> &[crate::ir::EdgeKind] {
        &[]
    }

    #[instrument(level = "trace", skip(self, node))]
    fn matches_node(&self, node: &dyn crate::ir::NodeView) -> bool {
        node.attr("visibility_rule_id").is_some()
    }
}

static VISIBILITY_SITES_QUERY: VisibilitySitesQuery = VisibilitySitesQuery;

/// Emits markers for visibility-path nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct VisibilitySiteProbe;

impl VisibilitySiteProbe {
    pub const ID: &'static str = "visibility-site";
}

impl Probe for VisibilitySiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &VISIBILITY_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, ir, _session))]
    fn probe(
        &self,
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Marker>>> {
        Ok(ir
            .nodes_matching(&VISIBILITY_SITES_QUERY)
            .into_iter()
            .map(|node| {
                Box::new(VisibilityMarker {
                    anchor: crate::objects::NodeAnchor(node.id),
                }) as Box<dyn Marker>
            })
            .collect())
    }
}
