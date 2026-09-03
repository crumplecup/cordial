use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{NodeKind, Query};
use crate::objects::Marker;

use super::types::{PRINT_SITE_LABEL, PrintMarker, PrintRuleId};

use tracing::instrument;

/// Matches leftover-print expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct PrintSitesQuery;

impl Query for PrintSitesQuery {
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
        node.attr("print_rule_id").is_some()
    }
}

static PRINT_SITES_QUERY: PrintSitesQuery = PrintSitesQuery;

/// Emits markers for leftover std print macros.
#[derive(Debug, Default, Clone, Copy)]
pub struct PrintSiteProbe;

impl PrintSiteProbe {
    pub const ID: &'static str = PRINT_SITE_LABEL;
}

impl Probe for PrintSiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &PRINT_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&PRINT_SITES_QUERY) {
            let Some(rule_value) = node.attr("print_rule_id").and_then(|v| v.as_str()) else {
                continue;
            };
            if PrintRuleId::from_attr(rule_value).is_none() {
                continue;
            }

            markers.push(
                Box::new(PrintMarker::new(crate::objects::NodeAnchor(node.id))) as Box<dyn Marker>,
            );
        }
        Ok(markers)
    }
}
