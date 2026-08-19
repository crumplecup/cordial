use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{NodeKind, Query};
use crate::objects::Marker;

use super::types::{ErrorChainMarker, ErrorChainProbeId};

use tracing::instrument;
/// Matches error-chain expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct ErrorChainQuery;

impl Query for ErrorChainQuery {
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
        node.attr("error_chain_rule_id").is_some()
    }
}

static ERROR_CHAIN_QUERY: ErrorChainQuery = ErrorChainQuery;

/// Emits markers for error-chain expression nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct ErrorChainProbe;

impl ErrorChainProbe {
    pub const ID: &'static str = "error-chain";
}

impl Probe for ErrorChainProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &ERROR_CHAIN_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&ERROR_CHAIN_QUERY) {
            let Some(rule_value) = node.attr("error_chain_rule_id").and_then(|v| v.as_str()) else {
                continue;
            };
            if ErrorChainProbeId::from_attr(rule_value).is_none() {
                continue;
            }

            markers.push(Box::new(ErrorChainMarker {
                anchor: crate::objects::NodeAnchor(node.id),
            }) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}
