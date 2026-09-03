use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{NodeKind, Query};
use crate::objects::Marker;

use super::types::{InternalErrorChainMarker, InternalErrorRecordKind};

use tracing::instrument;
/// Matches internal error-chain nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct InternalErrorChainQuery;

impl Query for InternalErrorChainQuery {
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
        node.attr("internal_error_record_kind").is_some()
    }
}

static INTERNAL_ERROR_CHAIN_QUERY: InternalErrorChainQuery = InternalErrorChainQuery;

/// Emits markers for internal error-chain nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct InternalErrorChainProbe;

impl InternalErrorChainProbe {
    pub const ID: &'static str = "internal-error-chain";
}

impl Probe for InternalErrorChainProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &INTERNAL_ERROR_CHAIN_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&INTERNAL_ERROR_CHAIN_QUERY) {
            let Some(kind_value) = node
                .attr("internal_error_record_kind")
                .and_then(|value| value.as_str())
            else {
                continue;
            };
            if InternalErrorRecordKind::from_attr(kind_value).is_none() {
                continue;
            }

            markers.push(
                Box::new(InternalErrorChainMarker::new(crate::objects::NodeAnchor(
                    node.id,
                ))) as Box<dyn Marker>,
            );
        }
        Ok(markers)
    }
}
