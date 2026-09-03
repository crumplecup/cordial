use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{NodeKind, Query};
use crate::objects::Marker;

use super::types::{CrateAttrsMarker, CrateAttrsRuleId};

use tracing::instrument;

/// Matches crate-attribute expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct CrateAttrsSitesQuery;

impl Query for CrateAttrsSitesQuery {
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
        node.attr("crate_attrs_rule_id").is_some()
    }
}

static CRATE_ATTRS_SITES_QUERY: CrateAttrsSitesQuery = CrateAttrsSitesQuery;

/// Emits markers for crate-attribute expression nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct CrateAttrsSiteProbe;

impl CrateAttrsSiteProbe {
    pub const ID: &'static str = "crate-attrs-site";
}

impl Probe for CrateAttrsSiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &CRATE_ATTRS_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&CRATE_ATTRS_SITES_QUERY) {
            let Some(rule_value) = node.attr("crate_attrs_rule_id").and_then(|v| v.as_str()) else {
                continue;
            };
            if CrateAttrsRuleId::from_attr(rule_value).is_none() {
                continue;
            }

            markers.push(
                Box::new(CrateAttrsMarker::new(crate::objects::NodeAnchor(node.id)))
                    as Box<dyn Marker>,
            );
        }
        Ok(markers)
    }
}
