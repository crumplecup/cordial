use crate::error::CordialResult;
use crate::hooks::Probe;
use crate::ir::{IrView, PanicSitesQuery, Query};
use crate::objects::Marker;
use crate::session::SessionView;

use super::types::{PanicKind, PanicMarker};

static PANIC_SITES_QUERY: PanicSitesQuery = PanicSitesQuery;

/// Emits markers for panic-site expression nodes in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct PanicSiteProbe;

impl PanicSiteProbe {
    pub const ID: &'static str = "panic-site";
}

impl Probe for PanicSiteProbe {
    fn id(&self) -> &str {
        Self::ID
    }

    fn interests(&self) -> &dyn Query {
        &PANIC_SITES_QUERY
    }

    fn probe(
        &self,
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Marker>>> {
        let mut markers = Vec::new();
        for node in ir.nodes_matching(&PANIC_SITES_QUERY) {
            let Some(kind_value) = node.attr("panic_kind").and_then(|v| v.as_str()) else {
                continue;
            };
            if PanicKind::from_attr(kind_value).is_none() {
                continue;
            }

            markers.push(Box::new(PanicMarker {
                anchor: crate::objects::NodeAnchor(node.id),
            }) as Box<dyn Marker>);
        }
        Ok(markers)
    }
}
