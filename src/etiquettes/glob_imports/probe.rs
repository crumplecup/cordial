use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{NodeKind, Query};
use crate::objects::Marker;

use super::types::{GlobImportMarker, GlobImportRuleId};

use tracing::instrument;

/// Matches glob-import expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct GlobImportSitesQuery;

impl Query for GlobImportSitesQuery {
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
        node.attr("glob_import_rule_id").is_some()
    }
}

static GLOB_IMPORT_SITES_QUERY: GlobImportSitesQuery = GlobImportSitesQuery;

/// Emits markers for glob-import expression nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct GlobImportSiteProbe;

impl GlobImportSiteProbe {
    pub const ID: &'static str = "glob-import-site";
}

impl Probe for GlobImportSiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &GLOB_IMPORT_SITES_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&GLOB_IMPORT_SITES_QUERY) {
            let Some(rule_value) = node.attr("glob_import_rule_id").and_then(|v| v.as_str()) else {
                continue;
            };
            if GlobImportRuleId::from_attr(rule_value).is_none() {
                continue;
            }

            markers.push(
                Box::new(GlobImportMarker::new(crate::objects::NodeAnchor(node.id)))
                    as Box<dyn Marker>,
            );
        }
        Ok(markers)
    }
}
