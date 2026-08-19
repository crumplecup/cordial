use crate::error::CordialResult;
use crate::hooks::{Probe, ProbeView};
use crate::ir::{NodeKind, Query};
use crate::objects::Marker;

use super::types::ForeignErrorAttenuationMarker;

use tracing::instrument;
#[derive(Debug, Default, Clone, Copy)]
struct ForeignErrorAttenuationQuery;

impl Query for ForeignErrorAttenuationQuery {
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
        node.attr("foreign_error_attenuation")
            .and_then(|value| value.as_bool())
            == Some(true)
    }
}

static FOREIGN_ERROR_ATTENUATION_QUERY: ForeignErrorAttenuationQuery = ForeignErrorAttenuationQuery;

#[derive(Debug, Default, Clone, Copy)]
pub struct ForeignErrorAttenuationProbe;

impl ForeignErrorAttenuationProbe {
    pub const ID: &'static str = "foreign-error-attenuation";
}

impl Probe for ForeignErrorAttenuationProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &FOREIGN_ERROR_ATTENUATION_QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir;

        let mut markers = Vec::new();
        for node in ir.nodes_matching(&FOREIGN_ERROR_ATTENUATION_QUERY) {
            markers.push(Box::new(ForeignErrorAttenuationMarker {
                anchor: crate::objects::NodeAnchor(node.id),
            }) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}
