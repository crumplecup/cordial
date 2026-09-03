use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{NodeKind, Query};
use crate::objects::Marker;

use super::types::{SUBSCRIBER_SITE_LABEL, SubscriberMarker, SubscriberRuleId};

use tracing::instrument;

/// Matches tracing-subscriber policy expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct SubscriberSitesQuery;

impl Query for SubscriberSitesQuery {
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
        node.attr("subscriber_rule_id").is_some()
    }
}

static SUBSCRIBER_SITES_QUERY: SubscriberSitesQuery = SubscriberSitesQuery;

/// Emits markers for tracing-subscriber policy nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct SubscriberSiteProbe;

impl SubscriberSiteProbe {
    pub const ID: &'static str = SUBSCRIBER_SITE_LABEL;
}

impl Probe for SubscriberSiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &SUBSCRIBER_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&SUBSCRIBER_SITES_QUERY) {
            let Some(rule_value) = node.attr("subscriber_rule_id").and_then(|v| v.as_str()) else {
                continue;
            };
            if SubscriberRuleId::from_attr(rule_value).is_none() {
                continue;
            }

            markers.push(
                Box::new(SubscriberMarker::new(crate::objects::NodeAnchor(node.id)))
                    as Box<dyn Marker>,
            );
        }
        Ok(markers)
    }
}
