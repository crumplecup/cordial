use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{NodeKind, Query};
use crate::objects::Marker;

use super::types::CliLayoutMarker;

use tracing::instrument;
/// Matches CLI-layout nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct CliLayoutSitesQuery;

impl Query for CliLayoutSitesQuery {
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
        node.attr("cli_layout_rule").is_some()
    }
}

static CLI_LAYOUT_SITES_QUERY: CliLayoutSitesQuery = CliLayoutSitesQuery;

/// Emits markers for CLI-layout nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct CliLayoutSiteProbe;

impl CliLayoutSiteProbe {
    pub const ID: &'static str = "cli-layout-site";
}

impl Probe for CliLayoutSiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &CLI_LAYOUT_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&CLI_LAYOUT_SITES_QUERY) {
            markers.push(
                Box::new(CliLayoutMarker::new(crate::objects::NodeAnchor(node.id)))
                    as Box<dyn Marker>,
            );
        }
        Ok(markers)
    }
}
