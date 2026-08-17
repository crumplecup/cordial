use crate::error::CordialResult;
use crate::framework_std::{
    FrameworkStdOptions, HOMECOMING_IMPL_CRATE, framework_std_type_items, load_merged_std_inventory,
};
use crate::hooks::Probe;
use crate::ir::{IrView, Query};
use crate::objects::Marker;
use crate::session::SessionView;
use crate::store::SysrootCache;

use super::types::FrameworkStdScopeMarker;

#[derive(Debug, Default, Clone, Copy)]
struct HubCrateQuery;

impl Query for HubCrateQuery {
    fn node_kinds(&self) -> &[crate::ir::NodeKind] {
        &[]
    }

    fn edge_kinds(&self) -> &[crate::ir::EdgeKind] {
        &[]
    }

    fn matches_node(&self, _node: &dyn crate::ir::NodeView) -> bool {
        true
    }
}

static HUB_CRATE_QUERY: HubCrateQuery = HubCrateQuery;

#[derive(Debug, Default, Clone, Copy)]
pub struct HomecomingStdScopeProbe;

impl HomecomingStdScopeProbe {
    pub const ID: &'static str = "homecoming-std-scope";
}

impl Probe for HomecomingStdScopeProbe {
    fn id(&self) -> &str {
        Self::ID
    }

    fn interests(&self) -> &dyn Query {
        &HUB_CRATE_QUERY
    }

    fn probe(
        &self,
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Marker>>> {
        if ir.crate_name() != HOMECOMING_IMPL_CRATE {
            return Ok(Vec::new());
        }

        let sysroot = SysrootCache::default_cache();
        let merged_items = load_merged_std_inventory(&sysroot)?;
        let options = FrameworkStdOptions::default();
        let anchor = crate::objects::NodeAnchor(ir.root()?);
        let probe_id = Self::ID.to_string();

        let markers = framework_std_type_items(&merged_items, options.include_nightly)
            .map(|item| {
                Box::new(FrameworkStdScopeMarker {
                    anchor,
                    probe_id: probe_id.clone(),
                    type_path: item.path.clone(),
                    type_kind: item.kind,
                    is_generic: item.is_generic,
                }) as Box<dyn Marker>
            })
            .collect();
        Ok(markers)
    }
}

#[cfg(feature = "amenable_std")]
mod amenable {
    use super::*;
    use crate::framework_std::{AMENABLE_IMPL_CRATE, AmenableStdOptions};

    #[derive(Debug, Default, Clone, Copy)]
    pub struct AmenableStdScopeProbe;

    impl AmenableStdScopeProbe {
        pub const ID: &'static str = "amenable-std-scope";
    }

    impl Probe for AmenableStdScopeProbe {
        fn id(&self) -> &str {
            Self::ID
        }

        fn interests(&self) -> &dyn Query {
            &HUB_CRATE_QUERY
        }

        fn probe(
            &self,
            ir: &dyn IrView,
            _session: &dyn SessionView,
        ) -> CordialResult<Vec<Box<dyn Marker>>> {
            if ir.crate_name() != AMENABLE_IMPL_CRATE {
                return Ok(Vec::new());
            }

            let sysroot = SysrootCache::default_cache();
            let merged_items = load_merged_std_inventory(&sysroot)?;
            let options = AmenableStdOptions::default();
            let anchor = crate::objects::NodeAnchor(ir.root()?);
            let probe_id = Self::ID.to_string();

            let markers = framework_std_type_items(&merged_items, options.include_nightly)
                .map(|item| {
                    Box::new(FrameworkStdScopeMarker {
                        anchor,
                        probe_id: probe_id.clone(),
                        type_path: item.path.clone(),
                        type_kind: item.kind,
                        is_generic: item.is_generic,
                    }) as Box<dyn Marker>
                })
                .collect();
            Ok(markers)
        }
    }
}

#[cfg(feature = "amenable_std")]
pub use amenable::AmenableStdScopeProbe;
