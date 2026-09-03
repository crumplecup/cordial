use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{NodeKind, Query};
use crate::objects::Marker;

use super::types::{VerusWarningMarker, VerusWarningRuleId};

use tracing::instrument;

/// Matches Verus compiler-warning expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct VerusWarningSitesQuery;

impl Query for VerusWarningSitesQuery {
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
        node.attr("verus_warning_rule_id").is_some()
    }
}

static VERUS_WARNING_SITES_QUERY: VerusWarningSitesQuery = VerusWarningSitesQuery;

/// Emits markers for Verus compiler-warning expression nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct VerusWarningSiteProbe;

impl VerusWarningSiteProbe {
    pub const ID: &'static str = "verus-warning-site";
}

impl Probe for VerusWarningSiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &VERUS_WARNING_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&VERUS_WARNING_SITES_QUERY) {
            let Some(rule_value) = node.attr("verus_warning_rule_id").and_then(|v| v.as_str())
            else {
                continue;
            };
            if VerusWarningRuleId::from_attr(rule_value).is_none() {
                continue;
            }

            markers.push(
                Box::new(VerusWarningMarker::new(crate::objects::NodeAnchor(node.id)))
                    as Box<dyn Marker>,
            );
        }
        Ok(markers)
    }
}
