use crate::error::CordialResult;
use crate::hooks::Probe;
use crate::ir::{IrView, NodeKind, Query};
use crate::objects::Marker;
use crate::session::SessionView;

use super::types::CfgScatterMarker;

/// Matches scattered-`cfg` group nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct CfgScatterSitesQuery;

impl Query for CfgScatterSitesQuery {
    fn node_kinds(&self) -> &[NodeKind] {
        &[NodeKind::Expr]
    }

    fn edge_kinds(&self) -> &[crate::ir::EdgeKind] {
        &[]
    }

    fn matches_node(&self, node: &dyn crate::ir::NodeView) -> bool {
        node.attr("cfg_scatter_predicate").is_some()
    }
}

static CFG_SCATTER_SITES_QUERY: CfgScatterSitesQuery = CfgScatterSitesQuery;

/// Emits markers for scattered-`cfg` group nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct CfgScatterSiteProbe;

impl CfgScatterSiteProbe {
    pub const ID: &'static str = "cfg-scatter-site";
}

impl Probe for CfgScatterSiteProbe {
    fn id(&self) -> &str {
        Self::ID
    }

    fn interests(&self) -> &dyn Query {
        &CFG_SCATTER_SITES_QUERY
    }

    fn probe(
        &self,
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Marker>>> {
        let mut markers = Vec::new();
        for node in ir.nodes_matching(&CFG_SCATTER_SITES_QUERY) {
            markers.push(Box::new(CfgScatterMarker {
                anchor: crate::objects::NodeAnchor(node.id),
            }) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}
