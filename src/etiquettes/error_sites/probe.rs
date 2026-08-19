use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{NodeKind, Query};
use crate::objects::Marker;

use super::types::{ErrorSiteKind, ErrorSiteMarker};

use tracing::instrument;
/// Matches error-site expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct ErrorSitesQuery;

impl Query for ErrorSitesQuery {
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
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &ERROR_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

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
