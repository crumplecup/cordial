use crate::error::CordialResult;
use crate::hooks::Probe;
use crate::ir::{IrView, NodeKind, Query};
use crate::objects::Marker;
use crate::session::SessionView;

use super::types::{AllowMarker, AllowRuleId};

use tracing::instrument;
/// Matches allow-attribute expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct AllowSitesQuery;

impl Query for AllowSitesQuery {
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
        node.attr("allow_rule_id").is_some()
    }
}

static ALLOW_SITES_QUERY: AllowSitesQuery = AllowSitesQuery;

/// Emits markers for allow-attribute expression nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct AllowSiteProbe;

impl AllowSiteProbe {
    pub const ID: &'static str = "allow-site";
}

impl Probe for AllowSiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &ALLOW_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, ir, _session))]
    fn probe(
        &self,
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Marker>>> {
        let mut markers = Vec::new();
        for node in ir.nodes_matching(&ALLOW_SITES_QUERY) {
            let Some(rule_value) = node.attr("allow_rule_id").and_then(|v| v.as_str()) else {
                continue;
            };
            if AllowRuleId::from_attr(rule_value).is_none() {
                continue;
            }

            markers.push(Box::new(AllowMarker {
                anchor: crate::objects::NodeAnchor(node.id),
            }) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}
