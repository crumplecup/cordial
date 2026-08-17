use crate::error::CordialResult;
use crate::hooks::Probe;
use crate::ir::{IrView, NodeKind, Query};
use crate::objects::Marker;
use crate::session::SessionView;

use super::types::{ErrorSiteKind, ErrorSiteMarker};

/// Matches error-site expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct ErrorSitesQuery;

impl Query for ErrorSitesQuery {
    fn node_kinds(&self) -> &[NodeKind] {
        &[NodeKind::Expr]
    }

    fn edge_kinds(&self) -> &[crate::ir::EdgeKind] {
        &[]
    }

    fn matches_node(&self, node: &dyn crate::ir::NodeView) -> bool {
        node.attr("error_site_kind").is_some()
    }
}

static ERROR_SITES_QUERY: ErrorSitesQuery = ErrorSitesQuery;

/// Emits markers for error-site expression nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct ErrorSiteProbe;

impl ErrorSiteProbe {
    pub const ID: &'static str = "error-site";
}

impl Probe for ErrorSiteProbe {
    fn id(&self) -> &str {
        Self::ID
    }

    fn interests(&self) -> &dyn Query {
        &ERROR_SITES_QUERY
    }

    fn probe(
        &self,
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Marker>>> {
        let mut markers = Vec::new();
        for node in ir.nodes_matching(&ERROR_SITES_QUERY) {
            let Some(kind_value) = node.attr("error_site_kind").and_then(|v| v.as_str()) else {
                continue;
            };
            if ErrorSiteKind::from_attr(kind_value).is_none() {
                continue;
            }

            markers.push(Box::new(ErrorSiteMarker {
                anchor: crate::objects::NodeAnchor(node.id),
            }) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}
