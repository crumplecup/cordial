//! Load a workspace member crate IR on demand (rustdoc + enrichers).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{CordialError, CordialResult};
use crate::hooks::{IrEnricher, Loader};
use crate::ir::{CrateIr, CrateViewMut, WorkspaceIr};
use crate::loader::{CrateTarget, LoadView, SourceLoadView, SourceLoader};
use crate::rustdoc::parse_rustdoc_json;
use crate::session::SessionView;
use crate::{RustdocLoadView, RustdocLoader};

use tracing::instrument;
/// Load and enrich one crate into `workspace` when missing.
#[instrument(level = "info", skip(workspace, session, loaders, enrichers), fields(crate_name = crate_name), err(level = "warn"))]
pub fn load_crate_ir_if_missing(
    workspace: &mut WorkspaceIr,
    session: &dyn SessionView,
    crate_name: &str,
    shadow_for_upstream: Option<&str>,
    loaders: &[&dyn Loader],
    enrichers: &[&dyn IrEnricher],
) -> CordialResult<()> {
    if workspace.crate_ir(crate_name).is_some() {
        return Ok(());
    }

    let target = resolve_crate_target(session.project_root(), crate_name);
    let mut crate_ir = CrateIr::new(crate_name);
    let mut load_views: HashMap<String, Box<dyn LoadView>> = HashMap::new();

    for loader in loaders {
        let view = if loader.id() == RustdocLoader::ID {
            load_rustdoc_view(session, &target, shadow_for_upstream)?
        } else {
            loader.load(session, &target)?
        };

        if view.loader_id() == SourceLoader::ID
            && let Some(source) = view.as_any().downcast_ref::<SourceLoadView>()
        {
            source.populate_ir(&mut crate_ir)?;
        }
        if view.loader_id() == RustdocLoader::ID
            && let Some(rustdoc) = view.as_any().downcast_ref::<RustdocLoadView>()
        {
            rustdoc.populate_ir(&mut crate_ir)?;
        }

        load_views
            .entry(format!("{}:{}", crate_name, loader.id()))
            .or_insert(view);
    }

    workspace.insert_crate(crate_ir);

    for enricher in enrichers {
        let preferred = enricher.required_loader();
        let load = load_views
            .get(&format!("{crate_name}:{preferred}"))
            .or_else(|| load_views.get(&format!("{crate_name}:{}", SourceLoader::ID)))
            .or_else(|| load_views.get(&format!("{crate_name}:{}", RustdocLoader::ID)))
            .map(|view| view.as_ref())
            .ok_or_else(|| {
                CordialError::invariant(format!(
                    "no load view for enricher `{}` on crate `{crate_name}`",
                    enricher.id()
                ))
            })?;
        let mut view = CrateViewMut {
            workspace,
            crate_name: crate_name.to_string(),
        };
        enricher.enrich(&mut view, load, session)?;
    }

    Ok(())
}

#[instrument(level = "info", skip(session, target), err(level = "warn"))]
pub fn load_rustdoc_view(
    session: &dyn SessionView,
    target: &CrateTarget,
    shadow_for_upstream: Option<&str>,
) -> CordialResult<Box<dyn LoadView>> {
    if let Some(shadow) = shadow_for_upstream
        && let Some(path) =
            shadow_dep_rustdoc_path(session.store_root(), shadow, &target.crate_name)
    {
        let inventory = parse_rustdoc_json(&path, &target.crate_name)?;
        return Ok(Box::new(RustdocLoadView::from_inventory(inventory)));
    }

    let json_path = crate::rustdoc_loader::resolve_rustdoc_json(
        &target.crate_root,
        &target.crate_name,
        Some(session.store_root()),
    )?;
    let inventory = parse_rustdoc_json(&json_path, &target.crate_name)?;
    Ok(Box::new(RustdocLoadView::from_inventory(inventory)))
}

#[instrument(level = "debug")]
pub fn shadow_dep_rustdoc_path(
    store_root: &Path,
    shadow_crate: &str,
    upstream_crate: &str,
) -> Option<PathBuf> {
    let path = store_root.join("cache").join("rustdoc").join(format!(
        "{}.json",
        crate::store::StoreLayout::shadow_dep_cache_stem(shadow_crate, upstream_crate)
    ));
    path.is_file().then_some(path)
}

#[instrument(level = "debug")]
pub fn resolve_crate_root(project_root: &Path, crate_name: &str) -> PathBuf {
    let member = project_root.join("crates").join(crate_name);
    if member.join("Cargo.toml").is_file() {
        member
    } else {
        project_root.to_path_buf()
    }
}

#[instrument(level = "debug")]
pub fn resolve_crate_target(project_root: &Path, crate_name: &str) -> CrateTarget {
    CrateTarget::new(crate_name, resolve_crate_root(project_root, crate_name))
}
