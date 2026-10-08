use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{NodeKind, Query};
use crate::objects::Marker;

use super::types::{FeatureWarningMarker, FeatureWarningRuleId};

use tracing::instrument;

/// Matches feature-warning expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct FeatureWarningSitesQuery;

impl Query for FeatureWarningSitesQuery {
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
        node.attr("feature_warning_rule_id").is_some()
    }
}

static FEATURE_WARNING_SITES_QUERY: FeatureWarningSitesQuery = FeatureWarningSitesQuery;

/// Emits markers for feature-warning expression nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct FeatureWarningSiteProbe;

impl FeatureWarningSiteProbe {
    /// Stable identifier for `FeatureWarningSiteProbe`.
    pub const ID: &'static str = "feature-warning-site";
}

impl Probe for FeatureWarningSiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &FEATURE_WARNING_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir();

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&FEATURE_WARNING_SITES_QUERY) {
            let Some(rule_value) = node
                .attr("feature_warning_rule_id")
                .and_then(|v| v.as_str())
            else {
                continue;
            };
            if FeatureWarningRuleId::from_attr(rule_value).is_none() {
                continue;
            }

            markers.push(
                Box::new(FeatureWarningMarker::new(crate::objects::NodeAnchor::new(
                    node.id(),
                ))) as Box<dyn Marker>,
            );
        }
        Ok(markers)
    }
}
