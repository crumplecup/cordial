use crate::error::CordialResult;
use crate::hooks::Probe;
use crate::ir::{ItemKind, NodeKind, Query};
use crate::objects::Marker;
use crate::session::SessionView;

use super::types::ImplGapMarker;

use tracing::instrument;
#[derive(Debug, Default, Clone, Copy)]
struct TypeNodesQuery;

impl Query for TypeNodesQuery {
    #[instrument(level = "trace", skip(self))]
    fn node_kinds(&self) -> &[NodeKind] {
        &[
            NodeKind::Item(ItemKind::Struct),
            NodeKind::Item(ItemKind::Enum),
        ]
    }

    #[instrument(level = "trace", skip(self))]
    fn edge_kinds(&self) -> &[crate::ir::EdgeKind] {
        &[]
    }

    #[instrument(level = "trace", skip(self, node))]
    fn matches_node(&self, node: &dyn crate::ir::NodeView) -> bool {
        node.attr("qualified_path").is_some()
    }
}

static TYPE_NODES_QUERY: TypeNodesQuery = TypeNodesQuery;

#[derive(Debug, Default, Clone, Copy)]
pub struct MissingPrereqProbe;

impl MissingPrereqProbe {
    pub const ID: &'static str = "impl-coverage-gap";
}

impl Probe for MissingPrereqProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &TYPE_NODES_QUERY
    }

    #[instrument(level = "trace", skip(self, ir, _session))]
    fn probe(
        &self,
        ir: &dyn crate::ir::IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Marker>>> {
        let mut markers = Vec::new();
        for node in ir.nodes_matching(&TYPE_NODES_QUERY) {
            if node
                .attr("qualified_path")
                .and_then(|v| v.as_str())
                .is_none()
            {
                continue;
            }
            markers.push(Box::new(ImplGapMarker {
                anchor: crate::objects::NodeAnchor(node.id),
            }) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}
