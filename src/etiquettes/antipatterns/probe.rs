use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{NodeKind, Query};
use crate::objects::Marker;

use super::types::{AntipatternMarker, AntipatternRuleId};

use tracing::instrument;
#[derive(Debug, Default, Clone, Copy)]
struct AntipatternSitesQuery;

impl Query for AntipatternSitesQuery {
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
        node.attr("antipattern_rule_id").is_some()
    }
}

static ANTIPATTERN_SITES_QUERY: AntipatternSitesQuery = AntipatternSitesQuery;

/// Emits markers for antipattern-site expression nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct AntipatternSiteProbe;

impl AntipatternSiteProbe {
    pub const ID: &'static str = "antipattern-site";
}

impl Probe for AntipatternSiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &ANTIPATTERN_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&ANTIPATTERN_SITES_QUERY) {
            let Some(rule_value) = node.attr("antipattern_rule_id").and_then(|v| v.as_str()) else {
                continue;
            };
            if AntipatternRuleId::from_attr(rule_value).is_none() {
                continue;
            }

            markers.push(
                Box::new(AntipatternMarker::new(crate::objects::NodeAnchor(node.id)))
                    as Box<dyn Marker>,
            );
        }
        Ok(markers)
    }
}
