use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{NodeKind, Query};
use crate::objects::Marker;

use super::types::{ForeignErrorRecordKind, ForeignErrorTypeMarker};

use tracing::instrument;
#[derive(Debug, Default, Clone, Copy)]
struct ForeignErrorTypeQuery;

impl Query for ForeignErrorTypeQuery {
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
        node.attr("foreign_error_record_kind").is_some()
    }
}

static FOREIGN_ERROR_TYPE_QUERY: ForeignErrorTypeQuery = ForeignErrorTypeQuery;

#[derive(Debug, Default, Clone, Copy)]
pub struct ForeignErrorTypeProbe;

impl ForeignErrorTypeProbe {
    pub const ID: &'static str = "foreign-error-type";
}

impl Probe for ForeignErrorTypeProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &FOREIGN_ERROR_TYPE_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&FOREIGN_ERROR_TYPE_QUERY) {
            let Some(kind_value) = node
                .attr("foreign_error_record_kind")
                .and_then(|value| value.as_str())
            else {
                continue;
            };
            if ForeignErrorRecordKind::from_attr(kind_value).is_none() {
                continue;
            }
            markers.push(Box::new(ForeignErrorTypeMarker {
                anchor: crate::objects::NodeAnchor(node.id),
            }) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}
