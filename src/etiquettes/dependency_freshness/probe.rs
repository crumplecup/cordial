use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{EdgeKind, NodeKind, NodeView, Query};
use crate::objects::Marker;

use super::types::{DependencyFreshnessMarker, DependencyFreshnessRuleId};

use tracing::instrument;

/// Matches dependency survey nodes with registry freshness rule ids.
#[derive(Debug, Default, Clone, Copy)]
pub struct DependencyFreshnessSitesQuery;

impl Query for DependencyFreshnessSitesQuery {
    #[instrument(level = "trace", skip(self))]
    fn node_kinds(&self) -> &[NodeKind] {
        &[]
    }

    #[instrument(level = "trace", skip(self))]
    fn edge_kinds(&self) -> &[EdgeKind] {
        &[]
    }

    #[instrument(level = "trace", skip(self, node))]
    fn matches_node(&self, node: &dyn NodeView) -> bool {
        node.attr("dependency_freshness_rule_ids")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| {
                value
                    .split('|')
                    .any(|rule_id| DependencyFreshnessRuleId::from_attr(rule_id).is_some())
            })
    }
}

static DEPENDENCY_FRESHNESS_SITES_QUERY: DependencyFreshnessSitesQuery =
    DependencyFreshnessSitesQuery;

/// Emits markers for dependency survey nodes with available dependency updates.
#[derive(Debug, Default, Clone, Copy)]
pub struct DependencyFreshnessSiteProbe;

impl DependencyFreshnessSiteProbe {
    pub const ID: &'static str = "dependency-freshness-site";
}

impl Probe for DependencyFreshnessSiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &DEPENDENCY_FRESHNESS_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        Ok(view
            .ir
            .nodes_matching(&DEPENDENCY_FRESHNESS_SITES_QUERY)
            .into_iter()
            .map(|node| {
                Box::new(DependencyFreshnessMarker::new(crate::objects::NodeAnchor(
                    node.id,
                ))) as Box<dyn Marker>
            })
            .collect())
    }
}
