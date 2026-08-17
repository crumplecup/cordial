use crate::error::CordialResult;
use crate::hooks::Probe;
use crate::ir::{IrView, NodeKind, Query};
use crate::objects::Marker;
use crate::session::SessionView;

use super::types::VisibilityMarker;

/// Matches visibility-path nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct VisibilitySitesQuery;

impl Query for VisibilitySitesQuery {
    fn node_kinds(&self) -> &[NodeKind] {
        &[NodeKind::Expr]
    }

    fn edge_kinds(&self) -> &[crate::ir::EdgeKind] {
        &[]
    }

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
    fn id(&self) -> &str {
        Self::ID
    }

    fn interests(&self) -> &dyn Query {
        &VISIBILITY_SITES_QUERY
    }

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
