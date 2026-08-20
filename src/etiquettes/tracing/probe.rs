use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{ItemKind, NodeKind, Query};
use crate::objects::Marker;

use super::types::{MISSING_INSTRUMENT_LABEL, RECIPE_DELTA_LABEL, TracingMarker};

use tracing::instrument;
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

/// Matches inventory functions that already have `#[instrument]`.
#[derive(Debug, Default, Clone, Copy)]
pub struct InstrumentedQuery;

impl Query for InstrumentedQuery {
    #[instrument(level = "trace", skip(self))]
    fn node_kinds(&self) -> &[NodeKind] {
        &[NodeKind::Item(ItemKind::Fn)]
    }

    #[instrument(level = "trace", skip(self))]
    fn edge_kinds(&self) -> &[crate::ir::EdgeKind] {
        &[]
    }

    #[instrument(level = "trace", skip(self, node))]
    fn matches_node(&self, node: &dyn crate::ir::NodeView) -> bool {
        node.attr("function_kind").is_some()
            && node
                .attr("instrumented")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
    }
}

static INSTRUMENTED_QUERY: InstrumentedQuery = InstrumentedQuery;

/// Emits markers for functions missing `#[instrument]`.
#[derive(Debug, Default, Clone, Copy)]
pub struct MissingInstrumentProbe;

impl MissingInstrumentProbe {
    pub const ID: &'static str = MISSING_INSTRUMENT_LABEL;
}

impl Probe for MissingInstrumentProbe {
    fn id(&self) -> &str {
        Self::ID
    }

    fn interests(&self) -> &dyn Query {
        &MISSING_INSTRUMENT_QUERY
    }

    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&MISSING_INSTRUMENT_QUERY) {
            markers.push(Box::new(TracingMarker {
                anchor: crate::objects::NodeAnchor(node.id),
                label: MISSING_INSTRUMENT_LABEL.to_string(),
            }) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}

/// Emits markers for instrumented functions to compare against the recipe.
#[derive(Debug, Default, Clone, Copy)]
pub struct RecipeDeltaProbe;

impl RecipeDeltaProbe {
    pub const ID: &'static str = RECIPE_DELTA_LABEL;
}

impl Probe for RecipeDeltaProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &INSTRUMENTED_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&INSTRUMENTED_QUERY) {
            markers.push(Box::new(TracingMarker {
                anchor: crate::objects::NodeAnchor(node.id),
                label: RECIPE_DELTA_LABEL.to_string(),
            }) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}
