use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{ItemKind, NodeKind, NodeView, Query};
use crate::objects::Marker;

use super::types::{
    FORBIDDEN_INSTRUMENT_LABEL, MISSING_INSTRUMENT_LABEL, RECIPE_DELTA_LABEL, TracingMarker,
};

use tracing::instrument;

#[instrument(level = "trace", skip(node), ret)]
fn is_inventory_fn(node: &dyn NodeView) -> bool {
    node.attr("function_kind").is_some()
}

#[instrument(level = "trace", skip(node), ret)]
fn bool_attr(node: &dyn NodeView, key: &str) -> bool {
    node.attr(key)
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

#[instrument(level = "trace", skip(node), ret)]
fn apply_policy(node: &dyn NodeView) -> &str {
    node.attr("tracing_apply_policy")
        .and_then(|value| value.as_str())
        .unwrap_or("bare")
}

/// `true` when an existing span must come off (proof-only / skip-policy)
/// or be gated (bare instrument on a prover-reachable ordinary function).
#[instrument(level = "trace", skip(node), ret)]
fn is_forbidden_instrument(node: &dyn NodeView) -> bool {
    if !bool_attr(node, "instrumented") {
        return false;
    }
    if bool_attr(node, "proof_only") {
        return true;
    }
    match apply_policy(node) {
        "skip" => true,
        "gated" => bool_attr(node, "prover_visible_instrument"),
        _ => false,
    }
}

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

    fn matches_node(&self, node: &dyn NodeView) -> bool {
        is_inventory_fn(node)
            && !bool_attr(node, "instrumented")
            && !bool_attr(node, "proof_only")
            && apply_policy(node) != "skip"
    }
}

static MISSING_INSTRUMENT_QUERY: MissingInstrumentQuery = MissingInstrumentQuery;

/// Matches inventory functions that already have a *kept* `#[instrument]`
/// (recipe-delta), not one attenuation will remove or rewrite.
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
    fn matches_node(&self, node: &dyn NodeView) -> bool {
        is_inventory_fn(node) && bool_attr(node, "instrumented") && !is_forbidden_instrument(node)
    }
}

static INSTRUMENTED_QUERY: InstrumentedQuery = InstrumentedQuery;

/// Matches inventory functions whose existing span is forbidden or ungated.
#[derive(Debug, Default, Clone, Copy)]
pub struct ForbiddenInstrumentQuery;

impl Query for ForbiddenInstrumentQuery {
    #[instrument(level = "trace", skip(self))]
    fn node_kinds(&self) -> &[NodeKind] {
        &[NodeKind::Item(ItemKind::Fn)]
    }

    #[instrument(level = "trace", skip(self))]
    fn edge_kinds(&self) -> &[crate::ir::EdgeKind] {
        &[]
    }

    #[instrument(level = "trace", skip(self, node))]
    fn matches_node(&self, node: &dyn NodeView) -> bool {
        is_inventory_fn(node) && is_forbidden_instrument(node)
    }
}

static FORBIDDEN_INSTRUMENT_QUERY: ForbiddenInstrumentQuery = ForbiddenInstrumentQuery;

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
            markers.push(Box::new(TracingMarker::new(
                crate::objects::NodeAnchor(node.id),
                MISSING_INSTRUMENT_LABEL.to_string(),
            )) as Box<dyn Marker>);
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
            markers.push(Box::new(TracingMarker::new(
                crate::objects::NodeAnchor(node.id),
                RECIPE_DELTA_LABEL.to_string(),
            )) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}

/// Emits markers for spans that must be removed or gated.
#[derive(Debug, Default, Clone, Copy)]
pub struct ForbiddenInstrumentProbe;

impl ForbiddenInstrumentProbe {
    pub const ID: &'static str = FORBIDDEN_INSTRUMENT_LABEL;
}

impl Probe for ForbiddenInstrumentProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &FORBIDDEN_INSTRUMENT_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&FORBIDDEN_INSTRUMENT_QUERY) {
            markers.push(Box::new(TracingMarker::new(
                crate::objects::NodeAnchor(node.id),
                FORBIDDEN_INSTRUMENT_LABEL.to_string(),
            )) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}
