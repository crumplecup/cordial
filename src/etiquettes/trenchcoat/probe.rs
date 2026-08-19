use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{EdgeKind, ItemKind, NodeKind, Query};
use crate::objects::Marker;

use super::types::UnwrappedMarker;

use tracing::instrument;
#[derive(Debug, Default, Clone, Copy)]
struct ForeignTypeQuery;

impl Query for ForeignTypeQuery {
    #[instrument(level = "trace", skip(self))]
    fn node_kinds(&self) -> &[NodeKind] {
        &[
            NodeKind::Item(ItemKind::Struct),
            NodeKind::Item(ItemKind::Enum),
        ]
    }

    #[instrument(level = "trace", skip(self))]
    fn edge_kinds(&self) -> &[EdgeKind] {
        &[]
    }

    #[instrument(level = "trace", skip(self, node))]
    fn matches_node(&self, node: &dyn crate::ir::NodeView) -> bool {
        node.attr("qualified_path")
            .and_then(|v| v.as_str())
            .is_some_and(|path| !is_wrapper_path(path))
    }
}

static FOREIGN_TYPE_QUERY: ForeignTypeQuery = ForeignTypeQuery;

#[derive(Debug, Default, Clone, Copy)]
pub struct UnwrappedForeignProbe;

impl UnwrappedForeignProbe {
    pub const ID: &'static str = "unwrapped-foreign";
}

impl Probe for UnwrappedForeignProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &FOREIGN_TYPE_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&FOREIGN_TYPE_QUERY) {
            let incoming_wraps = ir.parents(node.id, EdgeKind::Wraps);
            if !incoming_wraps.is_empty() {
                continue;
            }
            if node
                .attr("qualified_path")
                .and_then(|v| v.as_str())
                .is_none()
            {
                continue;
            }
            markers.push(Box::new(UnwrappedMarker {
                anchor: crate::objects::NodeAnchor(node.id),
            }) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}

#[instrument(level = "trace", skip(path), ret)]
fn is_wrapper_path(path: &str) -> bool {
    path.contains("Wrapper")
        || path.ends_with("Coat")
        || path.contains("Trenchcoat")
        || path.contains("Elicit")
}
