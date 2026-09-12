use crate::error::CordialResult;
use crate::framework_std::{
    FrameworkStdOptions, HOMECOMING_IMPL_CRATE, framework_std_type_items, load_merged_std_inventory,
};
use crate::hooks::{Probe, ProbeView};
use crate::ir::{BasicQuery, Query};
use crate::objects::Marker;
use crate::store::SysrootCache;

use super::homecoming::FrameworkStdScopeMarker;

use tracing::instrument;

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
        static QUERY: BasicQuery = BasicQuery::ALL_NODES;
        &QUERY
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir();

        if ir.crate_name() != HOMECOMING_IMPL_CRATE {
            return Ok(Vec::new());
        }

        let sysroot = SysrootCache::default_cache();
        let merged_items = load_merged_std_inventory(&sysroot)?;
        let options = FrameworkStdOptions::default();
        let anchor = crate::objects::NodeAnchor::new(ir.root()?);
        let probe_id = Self::ID.to_string();

        let markers = framework_std_type_items(&merged_items, options.include_nightly())
            .map(|item| {
                Box::new(FrameworkStdScopeMarker::new(
                    anchor,
                    probe_id.clone(),
                    item.path().clone(),
                    item.kind(),
                    item.is_generic(),
                )) as Box<dyn Marker>
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
    use crate::hooks::{Probe, ProbeView};
    use crate::ir::{BasicQuery, Query};
    use crate::objects::Marker;

    use crate::store::SysrootCache;
    use tracing::instrument;

    use super::FrameworkStdScopeMarker;

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
            static QUERY: BasicQuery = BasicQuery::ALL_NODES;
            &QUERY
        }

        #[instrument(level = "trace", skip(self, view))]
        fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
            let ir = view.ir();

            if ir.crate_name() != AMENABLE_IMPL_CRATE {
                return Ok(Vec::new());
            }

            let sysroot = SysrootCache::default_cache();
            let merged_items = load_merged_std_inventory(&sysroot)?;
            let options = AmenableStdOptions::default();
            let anchor = crate::objects::NodeAnchor::new(ir.root()?);
            let probe_id = Self::ID.to_string();

            let markers = framework_std_type_items(&merged_items, options.include_nightly())
                .map(|item| {
                    Box::new(FrameworkStdScopeMarker::new(
                        anchor,
                        probe_id.clone(),
                        item.path().clone(),
                        item.kind(),
                        item.is_generic(),
                    )) as Box<dyn Marker>
                })
                .collect();
            Ok(markers)
        }
    }
}

#[cfg(feature = "amenable_std")]
pub use amenable::AmenableStdScopeProbe;
