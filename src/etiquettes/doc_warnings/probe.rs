use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{NodeKind, Query};
use crate::objects::Marker;

use super::types::{DocWarningMarker, DocWarningRuleId};

use tracing::instrument;

/// Matches rustdoc-warning expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct DocWarningSitesQuery;

impl Query for DocWarningSitesQuery {
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
        node.attr("doc_warning_rule_id").is_some()
    }
}

static DOC_WARNING_SITES_QUERY: DocWarningSitesQuery = DocWarningSitesQuery;

/// Emits markers for rustdoc-warning expression nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct DocWarningSiteProbe;

impl DocWarningSiteProbe {
    /// Stable identifier for `DocWarningSiteProbe`.
    pub const ID: &'static str = "doc-warning-site";
}

impl Probe for DocWarningSiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &DOC_WARNING_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&DOC_WARNING_SITES_QUERY) {
            let Some(rule_value) = node.attr("doc_warning_rule_id").and_then(|v| v.as_str()) else {
                continue;
            };
            if DocWarningRuleId::from_attr(rule_value).is_none() {
                continue;
            }

            markers.push(
                Box::new(DocWarningMarker::new(crate::objects::NodeAnchor(node.id)))
                    as Box<dyn Marker>,
            );
        }
        Ok(markers)
    }
}
