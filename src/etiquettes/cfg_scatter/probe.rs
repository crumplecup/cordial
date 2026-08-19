use crate::error::CordialResult;
use crate::hooks::Probe;
use crate::ir::{IrView, NodeKind, Query};
use crate::objects::Marker;
use crate::session::SessionView;

use super::types::CfgScatterMarker;

use tracing::instrument;
/// Matches scattered-`cfg` group nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct CfgScatterSitesQuery;

impl Query for CfgScatterSitesQuery {
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
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &CFG_SCATTER_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, ir, _session))]
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
