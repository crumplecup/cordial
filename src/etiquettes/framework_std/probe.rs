use crate::error::CordialResult;
use crate::framework_std::{
    FrameworkStdOptions, HOMECOMING_IMPL_CRATE, framework_std_type_items, load_merged_std_inventory,
};
use crate::hooks::Probe;
use crate::ir::{IrView, Query};
use crate::objects::Marker;
use crate::session::SessionView;
use crate::store::SysrootCache;

use super::homecoming::FrameworkStdScopeMarker;

use tracing::instrument;
#[derive(Debug, Default, Clone, Copy)]
struct HubCrateQuery;

impl Query for HubCrateQuery {
    #[instrument(level = "trace", skip(self))]
    fn node_kinds(&self) -> &[crate::ir::NodeKind] {
        &[]
    }

    #[instrument(level = "trace", skip(self))]
    fn edge_kinds(&self) -> &[crate::ir::EdgeKind] {
        &[]
    }

    #[instrument(level = "trace", skip(self, _node))]
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
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn interests(&self) -> &dyn Query {
        &HUB_CRATE_QUERY
    }

    #[instrument(level = "trace", skip(self, ir, _session))]
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
    use crate::error::CordialResult;
    use crate::framework_std::{
        AMENABLE_IMPL_CRATE, AmenableStdOptions, framework_std_type_items,
        load_merged_std_inventory,
    };
    use crate::hooks::Probe;
    use crate::ir::{IrView, Query};
    use crate::objects::Marker;
    use crate::session::SessionView;
    use crate::store::SysrootCache;
    use tracing::instrument;

    use super::FrameworkStdScopeMarker;
    use super::HUB_CRATE_QUERY;

    #[derive(Debug, Default, Clone, Copy)]
    pub struct AmenableStdScopeProbe;

    impl AmenableStdScopeProbe {
        pub const ID: &'static str = "amenable-std-scope";
    }

    impl Probe for AmenableStdScopeProbe {
        #[instrument(level = "trace", skip(self))]
        fn id(&self) -> &str {
            Self::ID
        }

        #[instrument(level = "trace", skip(self))]
        fn interests(&self) -> &dyn Query {
            &HUB_CRATE_QUERY
        }

        #[instrument(level = "trace", skip(self, ir, _session))]
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
