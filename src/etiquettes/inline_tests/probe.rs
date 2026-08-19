use crate::error::CordialResult;
use crate::hooks::Probe;
use crate::ir::{IrView, NodeKind, Query};
use crate::objects::Marker;
use crate::session::SessionView;

use super::types::{InlineTestMarker, InlineTestRuleId};

use tracing::instrument;

/// Matches inline-test expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct InlineTestSitesQuery;

impl Query for InlineTestSitesQuery {
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
        node.attr("inline_test_rule_id").is_some()
    }
}

static INLINE_TEST_SITES_QUERY: InlineTestSitesQuery = InlineTestSitesQuery;

/// Emits markers for inline-test expression nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct InlineTestSiteProbe;

impl InlineTestSiteProbe {
    pub const ID: &'static str = "inline-test-site";
}

impl Probe for InlineTestSiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &INLINE_TEST_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, ir, _session))]
    fn probe(
        &self,
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Marker>>> {
        let mut markers = Vec::new();
        for node in ir.nodes_matching(&INLINE_TEST_SITES_QUERY) {
            let Some(rule_value) = node.attr("inline_test_rule_id").and_then(|v| v.as_str()) else {
                continue;
            };
            if InlineTestRuleId::from_attr(rule_value).is_none() {
                continue;
            }

            markers.push(Box::new(InlineTestMarker {
                anchor: crate::objects::NodeAnchor(node.id),
            }) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}
