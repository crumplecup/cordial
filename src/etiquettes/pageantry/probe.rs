use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{NodeKind, Query};
use crate::objects::Marker;

use super::types::{PageantryMarker, PageantryRuleId};

use tracing::instrument;

/// Matches pageantry expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct PageantrySitesQuery;

impl Query for PageantrySitesQuery {
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
        node.attr("pageantry_rule_id").is_some()
    }
}

static PAGEANTRY_SITES_QUERY: PageantrySitesQuery = PageantrySitesQuery;

/// Emits markers for pageantry expression nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct PageantrySiteProbe;

impl PageantrySiteProbe {
    pub const ID: &'static str = "pageantry-site";
}

impl Probe for PageantrySiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &PAGEANTRY_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&PAGEANTRY_SITES_QUERY) {
            let Some(rule_value) = node.attr("pageantry_rule_id").and_then(|v| v.as_str()) else {
                continue;
            };
            if PageantryRuleId::from_attr(rule_value).is_none() {
                continue;
            }

            markers.push(
                Box::new(PageantryMarker::new(crate::objects::NodeAnchor(node.id)))
                    as Box<dyn Marker>,
            );
        }
        Ok(markers)
    }
}
