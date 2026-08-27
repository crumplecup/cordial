use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{NodeKind, Query};
use crate::objects::Marker;

use super::types::{CfgHygieneMarker, CfgHygieneRuleId};

use tracing::instrument;
#[derive(Debug, Default, Clone, Copy)]
struct CfgHygieneSitesQuery;

impl Query for CfgHygieneSitesQuery {
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
        node.attr("cfg_hygiene_rule_id").is_some()
    }
}

static CFG_HYGIENE_SITES_QUERY: CfgHygieneSitesQuery = CfgHygieneSitesQuery;

/// Emits markers for cfg-hygiene-site expression nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct CfgHygieneSiteProbe;

impl CfgHygieneSiteProbe {
    pub const ID: &'static str = "cfg-hygiene-site";
}

impl Probe for CfgHygieneSiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &CFG_HYGIENE_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&CFG_HYGIENE_SITES_QUERY) {
            let Some(rule_value) = node.attr("cfg_hygiene_rule_id").and_then(|v| v.as_str()) else {
                continue;
            };
            if CfgHygieneRuleId::from_attr(rule_value).is_none() {
                continue;
            }

            markers.push(Box::new(CfgHygieneMarker {
                anchor: crate::objects::NodeAnchor(node.id),
            }) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}
