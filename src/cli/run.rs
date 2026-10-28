//! CLI command entry points and supporting build and inspection commands.
//!
//! `pipeline` runs checks and scores worklists. `apply` coordinates source
//! transformations and records the worklist items they fix.

use crate::{
    CordialError, CordialResult, CrateIr, Etiquette, NamedRunFilter, SessionBuilder, StoreLayout,
    SurrealGraphExport, all_plugins, default_store_home, etiquettes_from_plugins, lookup_etiquette,
};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tracing::instrument;

mod apply;
pub(super) use apply::execute_quality_apply;

mod pipeline;
pub(super) use pipeline::execute_run_plugins;
#[cfg(feature = "quality")]
pub(super) use pipeline::{RunOptions, execute_run_plugins_locked};

#[instrument(level = "debug", skip(store, progress), err(level = "warn"))]
#[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
pub(super) fn execute_build_rustdoc(
    project_root: &Path,
    store: &StoreLayout,
    crate_name: Option<&str>,
    progress: std::sync::Arc<dyn crate::ProgressSink>,
    force: bool,
) -> CordialResult<()> {
    let artifacts = crate::cargo_rustdoc::build_workspace_members_with_progress(
        project_root,
        store,
        crate_name,
        force,
        progress.as_ref(),
    )?;
    for artifact in artifacts {
        tracing::info!(
            crate_name = artifact.crate_name(),
            path = %artifact.rustdoc_json().display(),
            "built rustdoc"
        );
    }
    #[cfg(feature = "elicitation")]
    if let Ok(shadow_dep_artifacts) =
        crate::cargo_rustdoc::build_all_active_shadow_deps_with_progress(
            project_root,
            store,
            force,
            progress.as_ref(),
        )
    {
        for artifact in shadow_dep_artifacts {
            tracing::info!(
                crate_name = artifact.crate_name(),
                via = artifact.reference_member().as_deref().unwrap_or("unknown"),
                path = %artifact.rustdoc_json().display(),
                "built shadow-dep rustdoc"
            );
        }
    }
    Ok(())
}

#[instrument(level = "debug", skip(progress), err(level = "warn"))]
#[cfg(feature = "homecoming_std")]
pub(super) fn execute_build_sysroot(
    store_home: Option<PathBuf>,
    crate_name: Option<&str>,
    progress: std::sync::Arc<dyn crate::ProgressSink>,
    force: bool,
) -> CordialResult<()> {
    let home = store_home.unwrap_or_else(default_store_home);
    let sysroot = crate::SysrootCache::from_home(home);
    let artifacts = crate::cargo_rustdoc::build_sysroot_libraries_with_progress(
        &sysroot,
        crate_name,
        force,
        progress.as_ref(),
    )?;
    for artifact in artifacts {
        tracing::info!(
            crate_name = artifact.crate_name(),
            path = %sysroot.rustdoc_cache_path(artifact.crate_name()).display(),
            "built sysroot rustdoc"
        );
    }
    Ok(())
}

/// `cordial explain`: lists etiquettes for the selected project, so
/// etiquettes derived from its `cordial.toml` appear alongside built-ins.
#[instrument(level = "debug", err(level = "warn"))]
pub(super) fn execute_explain(
    project_root: &Path,
    store_home: Option<PathBuf>,
    id: Option<&str>,
) -> CordialResult<()> {
    let session = SessionBuilder::new(project_root)
        .with_store_home(store_home.unwrap_or_else(default_store_home))
        .build();
    let plugins = all_plugins();
    let owned = etiquettes_from_plugins(&plugins, &session);
    let etiquettes: Vec<&dyn Etiquette> =
        owned.iter().map(|etiquette| etiquette.as_ref()).collect();
    let config = crate::load_session_config(&session);
    match id {
        None => {
            write!(
                io::stdout(),
                "{}",
                super::tiers::render_explain_list_with_tiers(&etiquettes, &config)
            )?;
            Ok(())
        }
        Some(query) => {
            let Some(etiquette) = lookup_etiquette(&etiquettes, query) else {
                return Err(CordialError::unknown_etiquette(query));
            };
            write!(
                io::stdout(),
                "{}",
                super::tiers::render_explain_page_with_tier(etiquette, &config)
            )?;
            Ok(())
        }
    }
}

#[instrument(level = "debug", skip(store, path), err(level = "warn"))]
pub(super) fn view_store_file(store: &StoreLayout, path: &Path) -> CordialResult<()> {
    let full = store.root().join(path);
    if !full.is_file() {
        return Err(CordialError::not_found(full));
    }
    let bytes = fs::read(&full)?;
    io::stdout().write_all(&bytes)?;
    Ok(())
}

#[instrument(level = "debug", skip(store), err(level = "warn"))]
pub(super) fn export_surreal(
    store: &StoreLayout,
    project_root: &Path,
    crate_name: Option<&str>,
    output: Option<&Path>,
    statements: bool,
) -> CordialResult<()> {
    let crate_name = crate_name
        .map(str::to_string)
        .or_else(|| {
            crate::discover_crate_targets(project_root, &NamedRunFilter::all_plugins())
                .ok()
                .and_then(|targets| targets.into_iter().next().map(|t| t.crate_name().clone()))
        })
        .unwrap_or_else(|| store.project_slug().clone());
    let cache_path = store.ir_cache_path(&crate_name);
    if !cache_path.is_file() {
        return Err(CordialError::no_cached_ir(cache_path));
    }
    let ir = CrateIr::read_cache(&cache_path)?;
    let export = SurrealGraphExport::from_crate_ir(&ir)?;
    let body = if statements {
        crate::surreal_statements(&export).join("\n") + "\n"
    } else {
        export.to_json_pretty()?
    };

    if let Some(path) = output {
        fs::write(path, body)?;
        tracing::info!(path = %path.display(), "wrote surreal export");
    } else {
        write!(io::stdout(), "{body}")?;
    }
    Ok(())
}
