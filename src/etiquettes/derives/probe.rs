use crate::error::CordialResult;
use crate::hooks::Probe;
use crate::ir::{IrView, NodeKind, Query};
use crate::objects::Marker;
use crate::session::SessionView;

use super::types::{DeriveMarker, DeriveRuleId};

use tracing::instrument;
/// Matches derive-pattern expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct DeriveSitesQuery;

impl Query for DeriveSitesQuery {
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
        node.attr("derive_rule_id").is_some()
    }
}

static DERIVE_SITES_QUERY: DeriveSitesQuery = DeriveSitesQuery;

/// Emits markers for derive-pattern expression nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct DeriveSiteProbe;

impl DeriveSiteProbe {
    pub const ID: &'static str = "derive-site";
}

impl Probe for DeriveSiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &DERIVE_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, ir, _session))]
    fn probe(
        &self,
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Marker>>> {
        let mut markers = Vec::new();
        for node in ir.nodes_matching(&DERIVE_SITES_QUERY) {
            let Some(rule_value) = node.attr("derive_rule_id").and_then(|v| v.as_str()) else {
                continue;
            };
            if DeriveRuleId::from_attr(rule_value).is_none() {
                continue;
            }

            markers.push(Box::new(DeriveMarker {
                anchor: crate::objects::NodeAnchor(node.id),
            }) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}
