use std::path::{Path, PathBuf};

use crate::error::{CordialError, CordialResult};
use crate::hooks::{LoadContext, Loader};
use crate::ir::{
    ATTR_IS_PUBLIC, ATTR_ITEM_NAME, ATTR_QUALIFIED_PATH, ATTR_RUSTDOC_KIND, CrateIr, EdgeKind,
    NodeKind, NodeWeight,
};
use crate::loader::CrateTarget;
use crate::rustdoc::{RustdocInventory, ir_item_kind};
use crate::session::SessionView;
use crate::store::{StoreLayout, project_slug_from_path};

use tracing::instrument;
/// Loads parsed rustdoc JSON into a [`RustdocLoadView`].
#[derive(Debug, Default, Clone, Copy)]
pub struct RustdocLoader;

impl RustdocLoader {
    /// Stable identifier for `RustdocLoader`.
    pub const ID: &'static str = "rustdoc";
    /// IR attribute key (`crate_version`).
    pub const ATTR_CRATE_VERSION: &'static str = "crate_version";
    /// IR attribute key (`ir_origin`).
    pub const ATTR_IR_ORIGIN: &'static str = crate::ir::ATTR_IR_ORIGIN;
    /// Loader origin tag written onto IR nodes.
    pub const ORIGIN: &'static str = crate::ir::ORIGIN_RUSTDOC;
}

impl Loader for RustdocLoader {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn load(&self, view: LoadContext<'_>) -> CordialResult<Box<dyn crate::loader::LoadView>> {
        let session = view.session;
        let target = view.target;

        let json_path = resolve_or_rebuild_rustdoc_json(session, target)?;
        let inventory = crate::rustdoc::parse_rustdoc_json(&json_path, target.crate_name())?;
        Ok(Box::new(RustdocLoadView::from_inventory_path(
            inventory, json_path,
        )))
    }
}

/// Rustdoc loader output retained for the loader skeleton and structure enricher.
#[derive(Debug, Clone)]
pub struct RustdocLoadView {
    pub(crate) inventory: RustdocInventory,
    json_path: Option<PathBuf>,
}

impl RustdocLoadView {
    #[instrument(level = "debug", skip(inventory), ret)]
    pub(crate) fn from_inventory(inventory: RustdocInventory) -> Self {
        Self {
            inventory,
            json_path: None,
        }
    }

    #[instrument(level = "debug", skip(inventory, json_path), ret)]
    pub(crate) fn from_inventory_path(inventory: RustdocInventory, json_path: PathBuf) -> Self {
        Self {
            inventory,
            json_path: Some(json_path),
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub(crate) fn json_path(&self) -> Option<&Path> {
        self.json_path.as_deref()
    }
}

impl crate::loader::LoadView for RustdocLoadView {
    #[instrument(level = "trace", skip(self))]
    fn loader_id(&self) -> &str {
        RustdocLoader::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn crate_name(&self) -> &str {
        &self.inventory.crate_name
    }

    #[instrument(level = "trace", skip(self))]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl RustdocLoadView {
    /// Fill the crate IR from loaded views.
    #[instrument(level = "debug", skip(self, ir), err(level = "warn"))]
    pub fn populate_ir(&self, ir: &mut CrateIr) -> CordialResult<()> {
        let root = ir.root;
        ir.set_attr(
            root,
            RustdocLoader::ATTR_CRATE_VERSION,
            serde_json::Value::String(self.inventory.crate_version.clone()),
        )?;
        for item in &self.inventory.items {
            if !item.kind.is_type() && item.kind != crate::rustdoc::InventoryItemKind::Trait {
                continue;
            }
            let node = ir.insert_node(
                NodeWeight::new(NodeKind::Item(ir_item_kind(item.kind)))
                    .with_name(item.name.clone()),
            );
            ir.set_attr(
                node,
                ATTR_QUALIFIED_PATH,
                serde_json::Value::String(item.path.clone()),
            )?;
            ir.set_attr(
                node,
                ATTR_RUSTDOC_KIND,
                serde_json::Value::String(format!("{:?}", item.kind)),
            )?;
            ir.set_attr(
                node,
                ATTR_ITEM_NAME,
                serde_json::Value::String(item.name.clone()),
            )?;
            ir.set_attr(
                node,
                ATTR_IS_PUBLIC,
                serde_json::Value::Bool(item.is_public),
            )?;
            ir.set_attr(
                node,
                RustdocLoader::ATTR_IR_ORIGIN,
                serde_json::Value::String(RustdocLoader::ORIGIN.to_string()),
            )?;
            ir.insert_edge(root, node, EdgeKind::Contains)?;
        }
        Ok(())
    }
}

/// Resolve rustdoc json.
#[instrument(level = "debug", err(level = "warn"))]
pub fn resolve_rustdoc_json(
    crate_root: &Path,
    crate_name: &str,
    store_root: Option<&Path>,
) -> CordialResult<PathBuf> {
    find_rustdoc_json(crate_root, crate_name, store_root)
        .ok_or_else(|| CordialError::missing_rustdoc_json(crate_name, crate_root.to_path_buf()))
}

/// Resolve rustdoc JSON, rebuilding the workspace-member cache when absent or stale.
#[instrument(level = "info", skip(session, target), fields(crate_name = target.crate_name()), err(level = "warn"))]
pub(crate) fn resolve_or_rebuild_rustdoc_json(
    session: &dyn SessionView,
    target: &CrateTarget,
) -> CordialResult<PathBuf> {
    if let Some(path) = find_rustdoc_json(
        target.crate_root(),
        target.crate_name(),
        Some(session.store_root()),
    ) && crate::cargo_rustdoc::rustdoc_cache_is_fresh(
        session.project_root(),
        target.crate_root(),
        &path,
    ) {
        return Ok(path);
    }

    tracing::info!(
        crate_name = %target.crate_name(),
        "rebuilding missing or stale rustdoc cache"
    );
    let store = StoreLayout::from_root(
        session.store_root(),
        project_slug_from_path(session.project_root()),
    );
    crate::cargo_rustdoc::build_workspace_members_with_progress(
        session.project_root(),
        &store,
        Some(target.crate_name()),
        true,
        session.progress(),
    )?;

    resolve_rustdoc_json(
        target.crate_root(),
        target.crate_name(),
        Some(session.store_root()),
    )
}

/// Find rustdoc JSON when it is available.
#[instrument(level = "debug")]
pub(crate) fn find_rustdoc_json(
    crate_root: &Path,
    crate_name: &str,
    store_root: Option<&Path>,
) -> Option<PathBuf> {
    let normalized = crate_name.replace('-', "_");
    let candidates = [
        crate_root.join("doc").join(format!("{normalized}.json")),
        crate_root
            .join("target")
            .join("doc")
            .join(format!("{normalized}.json")),
        crate_root.join(format!("{normalized}.rustdoc.json")),
    ];
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .or_else(|| store_rustdoc_candidate(store_root, crate_name))
}

#[instrument(level = "debug")]
fn store_rustdoc_candidate(store_root: Option<&Path>, crate_name: &str) -> Option<PathBuf> {
    let store_root = store_root?;
    let path = store_root
        .join("cache")
        .join("rustdoc")
        .join(format!("{crate_name}.json"));
    path.is_file().then_some(path)
}
