use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{NodeKind, Query};
use crate::objects::Marker;

use super::types::{BOUNDARY_SITE_LABEL, BoundaryMarker, BoundaryRuleId};

use tracing::instrument;

/// Matches binary-error-boundary policy expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct BoundarySitesQuery;

impl Query for BoundarySitesQuery {
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
        node.attr("boundary_rule_id").is_some()
    }
}

static BOUNDARY_SITES_QUERY: BoundarySitesQuery = BoundarySitesQuery;

/// Emits markers for binary-error-boundary policy nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct BoundarySiteProbe;

impl BoundarySiteProbe {
    pub const ID: &'static str = BOUNDARY_SITE_LABEL;
}

impl Probe for BoundarySiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &BOUNDARY_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&BOUNDARY_SITES_QUERY) {
            let Some(rule_value) = node.attr("boundary_rule_id").and_then(|v| v.as_str()) else {
                continue;
            };
            if BoundaryRuleId::from_attr(rule_value).is_none() {
                continue;
            }

            markers.push(Box::new(BoundaryMarker {
                anchor: crate::objects::NodeAnchor(node.id),
            }) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}
