use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{NodeKind, Query};
use crate::objects::Marker;

use super::types::{CreusotDiagnosticMarker, CreusotDiagnosticRuleId};

use tracing::instrument;

/// Matches Creusot diagnostic expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct CreusotDiagnosticSitesQuery;

impl Query for CreusotDiagnosticSitesQuery {
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
        node.attr("creusot_diagnostic_rule_id").is_some()
    }
}

static CREUSOT_DIAGNOSTIC_SITES_QUERY: CreusotDiagnosticSitesQuery = CreusotDiagnosticSitesQuery;

/// Emits markers for Creusot diagnostic expression nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct CreusotDiagnosticSiteProbe;

impl CreusotDiagnosticSiteProbe {
    pub const ID: &'static str = "creusot-diagnostic-site";
}

impl Probe for CreusotDiagnosticSiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &CREUSOT_DIAGNOSTIC_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&CREUSOT_DIAGNOSTIC_SITES_QUERY) {
            let Some(rule_value) = node
                .attr("creusot_diagnostic_rule_id")
                .and_then(|v| v.as_str())
            else {
                continue;
            };
            if CreusotDiagnosticRuleId::from_attr(rule_value).is_none() {
                continue;
            }

            markers.push(
                Box::new(CreusotDiagnosticMarker::new(crate::objects::NodeAnchor(
                    node.id,
                ))) as Box<dyn Marker>,
            );
        }
        Ok(markers)
    }
}
