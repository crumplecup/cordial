use crate::error::CordialResult;
use crate::hooks::Probe;
use crate::ir::{IrView, NodeKind, Query};
use crate::objects::Marker;
use crate::session::SessionView;

use super::types::{ModularityKind, ModularityMarker};

use tracing::instrument;
/// Matches modularity-site expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct ModularitySitesQuery;

impl Query for ModularitySitesQuery {
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
        node.attr("modularity_kind").is_some()
    }
}

static MODULARITY_SITES_QUERY: ModularitySitesQuery = ModularitySitesQuery;

/// Emits markers for modularity-site expression nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct ModularitySiteProbe;

impl ModularitySiteProbe {
    pub const ID: &'static str = "modularity-site";
}

impl Probe for ModularitySiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &MODULARITY_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, ir, _session))]
    fn probe(
        &self,
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Marker>>> {
        let mut markers = Vec::new();
        for node in ir.nodes_matching(&MODULARITY_SITES_QUERY) {
            let Some(kind_value) = node.attr("modularity_kind").and_then(|v| v.as_str()) else {
                continue;
            };
            if ModularityKind::from_attr(kind_value).is_none() {
                continue;
            }

            markers.push(Box::new(ModularityMarker {
                anchor: crate::objects::NodeAnchor(node.id),
            }) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}
