//! Scope probe for amenable ext (third-party crate) registry coverage —
//! one generic implementation shared by every `amenable-ext-{target}`
//! etiquette instance, parameterized by the upstream crate name.

use crate::error::CordialResult;
use crate::framework_std::{
    AMENABLE_EXT_IMPL_CRATE, AmenableExtOptions, framework_std_type_items,
    load_ext_inventory_from_shadow_dep,
};
use crate::hooks::{Probe, ProbeView};
use crate::ir::{BasicQuery, Query};
use crate::objects::{IrAnchor, Marker, NodeAnchor, SourceSpan};
use crate::rustdoc::InventoryItemKind;
use crate::store::StoreLayout;

use tracing::instrument;

/// One inventory row in scope for amenable_ext registry coverage.
#[derive(Debug, Clone, derive_new::new)]
pub struct ExtScopeMarker {
    anchor: NodeAnchor,
    probe_id: String,
    type_path: String,
    type_kind: InventoryItemKind,
    is_generic: bool,
}

impl Marker for ExtScopeMarker {
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

/// Scope probe for one amenable-ext target crate's inventory.
#[derive(Debug, Clone)]
pub struct ExtScopeProbe {
    id: String,
    upstream_crate: String,
}

impl ExtScopeProbe {
    /// Build the scope probe for `target` (e.g. `"jiff"`).
    #[instrument(level = "debug")]
    pub fn new(target: &str) -> Self {
        Self {
            id: format!("amenable-ext-{target}-scope"),
            upstream_crate: target.to_string(),
        }
    }
}

impl Probe for ExtScopeProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        &self.id
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
            &self.upstream_crate,
            options.force_rustdoc(),
        )?;
        let anchor = NodeAnchor::new(ir.root()?);
        let probe_id = self.id.clone();

        let markers = framework_std_type_items(&items, options.include_nightly())
            .map(|item| {
                Box::new(ExtScopeMarker::new(
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
