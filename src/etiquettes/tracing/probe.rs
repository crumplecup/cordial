use crate::error::CordialResult;
use crate::hooks::Probe;
use crate::ir::{IrView, ItemKind, NodeKind, Query};
use crate::objects::Marker;
use crate::session::SessionView;

use super::types::TracingMarker;

/// Matches traced function inventory nodes missing `#[instrument]`.
#[derive(Debug, Default, Clone, Copy)]
pub struct MissingInstrumentQuery;

impl Query for MissingInstrumentQuery {
    fn node_kinds(&self) -> &[NodeKind] {
        &[NodeKind::Item(ItemKind::Fn)]
    }

    fn edge_kinds(&self) -> &[crate::ir::EdgeKind] {
        &[]
    }

    fn matches_node(&self, node: &dyn crate::ir::NodeView) -> bool {
        if node.attr("function_kind").is_none() {
            return false;
        }
        !node
            .attr("instrumented")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    }
}

static MISSING_INSTRUMENT_QUERY: MissingInstrumentQuery = MissingInstrumentQuery;

/// Emits markers for functions missing `#[instrument]`.
#[derive(Debug, Default, Clone, Copy)]
pub struct MissingInstrumentProbe;

impl MissingInstrumentProbe {
    pub const ID: &'static str = "missing-instrument";
}

impl Probe for MissingInstrumentProbe {
    fn id(&self) -> &str {
        Self::ID
    }

    fn interests(&self) -> &dyn Query {
        &MISSING_INSTRUMENT_QUERY
    }

    fn probe(
        &self,
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Marker>>> {
        let mut markers = Vec::new();
        for node in ir.nodes_matching(&MISSING_INSTRUMENT_QUERY) {
            markers.push(Box::new(TracingMarker {
                anchor: crate::objects::NodeAnchor(node.id),
            }) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}
