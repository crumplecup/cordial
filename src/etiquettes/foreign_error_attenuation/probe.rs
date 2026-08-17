use crate::error::CordialResult;
use crate::hooks::Probe;
use crate::ir::{IrView, NodeKind, Query};
use crate::objects::Marker;
use crate::session::SessionView;

use super::types::ForeignErrorAttenuationMarker;

#[derive(Debug, Default, Clone, Copy)]
struct ForeignErrorAttenuationQuery;

impl Query for ForeignErrorAttenuationQuery {
    fn node_kinds(&self) -> &[NodeKind] {
        &[NodeKind::Expr]
    }

    fn edge_kinds(&self) -> &[crate::ir::EdgeKind] {
        &[]
    }

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
    fn id(&self) -> &str {
        Self::ID
    }

    fn interests(&self) -> &dyn Query {
        &FOREIGN_ERROR_ATTENUATION_QUERY
    }

    fn probe(
        &self,
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Marker>>> {
        let mut markers = Vec::new();
        for node in ir.nodes_matching(&FOREIGN_ERROR_ATTENUATION_QUERY) {
            markers.push(Box::new(ForeignErrorAttenuationMarker {
                anchor: crate::objects::NodeAnchor(node.id),
            }) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}
