use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{NodeKind, Query};
use crate::objects::Marker;

use super::types::{ProofPatternKind, ProofPatternMarker};

use tracing::instrument;

/// Matches proof-pattern expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProofPatternSitesQuery;

impl Query for ProofPatternSitesQuery {
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
        node.attr("proof_pattern_kind").is_some()
    }
}

static PROOF_PATTERN_SITES_QUERY: ProofPatternSitesQuery = ProofPatternSitesQuery;

/// Emits markers for proof-pattern expression nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProofPatternSiteProbe;

impl ProofPatternSiteProbe {
    pub const ID: &'static str = "proof-pattern-site";
}

impl Probe for ProofPatternSiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &PROOF_PATTERN_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&PROOF_PATTERN_SITES_QUERY) {
            let Some(kind_value) = node.attr("proof_pattern_kind").and_then(|v| v.as_str()) else {
                continue;
            };
            if ProofPatternKind::from_attr(kind_value).is_none() {
                continue;
            }

            markers.push(Box::new(ProofPatternMarker {
                anchor: crate::objects::NodeAnchor(node.id),
            }) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}
