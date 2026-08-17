use crate::error::CordialResult;
use crate::hooks::Probe;
use crate::ir::{IrView, NodeKind, Query};
use crate::objects::Marker;
use crate::session::SessionView;

use super::types::{AllowMarker, AllowRuleId};

/// Matches allow-attribute expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct AllowSitesQuery;

impl Query for AllowSitesQuery {
    fn node_kinds(&self) -> &[NodeKind] {
        &[NodeKind::Expr]
    }

    fn edge_kinds(&self) -> &[crate::ir::EdgeKind] {
        &[]
    }

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
    fn id(&self) -> &str {
        Self::ID
    }

    fn interests(&self) -> &dyn Query {
        &ALLOW_SITES_QUERY
    }

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
