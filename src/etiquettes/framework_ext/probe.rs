//! Scope probe for amenable ext (jiff) registry coverage — the
//! `amenable_ext` counterpart of `etiquettes::framework_std::probe`'s
//! `AmenableStdScopeProbe`, which does the identical job for
//! `amenable_std` against the shared sysroot inventory instead of a
//! shadow-dep-built one.

use crate::error::CordialResult;
use crate::framework_std::{
    AMENABLE_EXT_IMPL_CRATE, AMENABLE_EXT_JIFF_UPSTREAM_CRATE, AmenableExtOptions,
    framework_std_type_items, load_ext_inventory_from_shadow_dep,
};
use crate::hooks::{Probe, ProbeView};
use crate::ir::{BasicQuery, Query};
use crate::objects::{IrAnchor, Marker, NodeAnchor, SourceSpan};
use crate::rustdoc::InventoryItemKind;
use crate::store::StoreLayout;

use tracing::instrument;

/// One jiff inventory row in scope for amenable_ext registry coverage.
#[derive(Debug, Clone, derive_new::new)]
pub struct AmenableExtJiffScopeMarker {
    anchor: NodeAnchor,
    probe_id: String,
    type_path: String,
    type_kind: InventoryItemKind,
    is_generic: bool,
}

impl Marker for AmenableExtJiffScopeMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        &self.probe_id
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        &self.probe_id
    }

    #[instrument(level = "trace", skip(self))]
    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    #[instrument(level = "trace", skip(self))]
    fn span(&self) -> Option<&dyn SourceSpan> {
        None
    }

    #[instrument(level = "trace", skip(self))]
    fn field(&self, key: &str) -> Option<&str> {
        match key {
            "type_path" => Some(&self.type_path),
            "type_kind" => Some(self.type_kind.as_str()),
            "is_generic" => Some(if self.is_generic { "true" } else { "false" }),
            _ => None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AmenableExtJiffScopeProbe;

impl AmenableExtJiffScopeProbe {
    pub const ID: &'static str = "amenable-ext-jiff-scope";
}

impl Probe for AmenableExtJiffScopeProbe {
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

        if ir.crate_name() != AMENABLE_EXT_IMPL_CRATE {
            return Ok(Vec::new());
        }

        let session = view.session();
        let store = StoreLayout::from_root(
            session.store_root(),
            crate::store::project_slug_from_path(session.project_root()),
        );
        let options = AmenableExtOptions::default();
        let items = load_ext_inventory_from_shadow_dep(
            session.project_root(),
            &store,
            AMENABLE_EXT_IMPL_CRATE,
            AMENABLE_EXT_JIFF_UPSTREAM_CRATE,
            options.force_rustdoc(),
        )?;
        let anchor = NodeAnchor::new(ir.root()?);
        let probe_id = Self::ID.to_string();

        let markers = framework_std_type_items(&items, options.include_nightly())
            .map(|item| {
                Box::new(AmenableExtJiffScopeMarker::new(
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
