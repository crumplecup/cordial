use crate::error::CordialResult;
use crate::hooks::Probe;
use crate::ir::{IrView, NodeKind, Query};
use crate::objects::Marker;
use crate::session::SessionView;

use super::types::{ModularityKind, ModularityMarker};

/// Matches modularity-site expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct ModularitySitesQuery;

impl Query for ModularitySitesQuery {
    fn node_kinds(&self) -> &[NodeKind] {
        &[NodeKind::Expr]
    }

    fn edge_kinds(&self) -> &[crate::ir::EdgeKind] {
        &[]
    }

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
    fn id(&self) -> &str {
        Self::ID
    }

    fn interests(&self) -> &dyn Query {
        &MODULARITY_SITES_QUERY
    }

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
