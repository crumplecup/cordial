//! Build upstream rustdoc through shadow-member dependency feature edges.

use std::path::{Path, PathBuf};

use tracing::instrument;

use crate::cargo_rustdoc::{DepBuildConfig, collect_member_dep_build_config};
use crate::error::CordialResult;
use crate::plugin::{discover_active_shadow_pairs, tracked_target_for_shadow};
use crate::progress::{ProgressSink, noop_progress};
use crate::session::{RunAll, RunFilter};
use crate::store::StoreLayout;

use super::artifact::BuildArtifact;
use super::cargo::{run_cargo_doc_for_optional_dep, run_cargo_rustdoc};
use super::{
    copy_rustdoc_json, hash_file, read_build_artifact, read_crate_version, rustdoc_cache_is_fresh,
    write_build_artifact,
};

/// Resolve how to build upstream rustdoc for one shadow mirror pair.
#[instrument(level = "debug")]
pub fn resolve_shadow_dep_build_config(
    project_root: &Path,
    shadow_crate: &str,
    upstream_crate: &str,
) -> DepBuildConfig {
    if let Ok(config) = collect_member_dep_build_config(project_root, shadow_crate, upstream_crate)
    {
        return config;
    }

    tracked_target_for_shadow(shadow_crate)
        .filter(|target| target.upstream() == upstream_crate)
        .map(|target| {
            DepBuildConfig::new(
                target
                    .impl_dep_features()
                    .iter()
                    .map(|feature| (*feature).to_string())
                    .collect(),
                true,
                None,
            )
        })
        .unwrap_or_default()
}

/// Build and cache upstream rustdoc for one shadow ↔ upstream pair.
#[instrument(level = "debug", skip(store), err(level = "warn"))]
pub fn build_shadow_dep_rustdoc(
    project_root: &Path,
    store: &StoreLayout,
    shadow_crate: &str,
    upstream_crate: &str,
    force: bool,
) -> CordialResult<BuildArtifact> {
    build_shadow_dep_rustdoc_with_progress(
        project_root,
        store,
        shadow_crate,
        upstream_crate,
        force,
        noop_progress(),
    )
}

#[instrument(level = "debug", skip(store, progress), err(level = "warn"))]
pub(crate) fn build_shadow_dep_rustdoc_with_progress(
    project_root: &Path,
    store: &StoreLayout,
    shadow_crate: &str,
    upstream_crate: &str,
    force: bool,
    progress: &dyn ProgressSink,
) -> CordialResult<BuildArtifact> {
    store.ensure_dirs()?;
    std::fs::create_dir_all(store.builds_dir())?;
    std::fs::create_dir_all(store.rustdoc_cache_dir())?;

    let artifact_path = store.shadow_dep_build_artifact_path(shadow_crate, upstream_crate);
    let cached_json = store.shadow_dep_rustdoc_cache_path(shadow_crate, upstream_crate);

    if !force
        && artifact_path.is_file()
        && cached_json.is_file()
        && rustdoc_cache_is_fresh(
            project_root,
            &reference_member_root(project_root, shadow_crate),
            &cached_json,
        )
        && let Ok(existing) = read_build_artifact(&artifact_path)
    {
        return Ok(existing);
    }

    let dep_config = resolve_shadow_dep_build_config(project_root, shadow_crate, upstream_crate);
    let task = progress.spinner(format!("Building rustdoc for shadow dep {upstream_crate}"));
    let json_path = if let Some(activating_feature) = dep_config.activating_member_feature() {
        run_cargo_doc_for_optional_dep(
            project_root,
            shadow_crate,
            activating_feature,
            upstream_crate,
        )?
    } else {
        let feature_refs: Vec<&str> = dep_config
            .activated_features()
            .iter()
            .map(String::as_str)
            .collect();
        run_cargo_rustdoc(project_root, upstream_crate, &feature_refs)?
    };
    copy_rustdoc_json(&json_path, &cached_json)?;

    let rustdoc_sha256 = hash_file(&cached_json)?;
    let crate_version = read_crate_version(&cached_json).unwrap_or_else(|| "unknown".to_string());
    let relative_json = PathBuf::from(format!(
        "cache/rustdoc/{}.json",
        StoreLayout::shadow_dep_cache_stem(shadow_crate, upstream_crate)
    ));
    let artifact = BuildArtifact::shadow_dep(
        shadow_crate,
        upstream_crate,
        relative_json,
        dep_config.activated_features().clone(),
        dep_config.uses_default_features(),
        super::artifact::DocFingerprint::new(rustdoc_sha256, crate_version),
    );
    write_build_artifact(&artifact_path, &artifact)?;
    task.finish(format!(
        "Shadow-dep rustdoc cache ready for {upstream_crate}"
    ));
    Ok(artifact)
}

#[instrument(level = "debug")]
fn reference_member_root(project_root: &Path, member_crate: &str) -> PathBuf {
    let crate_root = project_root.join("crates").join(member_crate);
    if crate_root.join("Cargo.toml").is_file() {
        crate_root
    } else {
        project_root.to_path_buf()
    }
}

/// Build shadow-dep rustdoc for every active tracked pair in the workspace.
#[instrument(level = "debug", skip(store, filter), err(level = "warn"))]
pub fn build_active_shadow_deps(
    project_root: &Path,
    store: &StoreLayout,
    filter: &dyn RunFilter,
    force: bool,
) -> CordialResult<Vec<BuildArtifact>> {
    build_active_shadow_deps_with_progress(project_root, store, filter, force, noop_progress())
}

#[instrument(level = "debug", skip(store, filter, progress), err(level = "warn"))]
pub(crate) fn build_active_shadow_deps_with_progress(
    project_root: &Path,
    store: &StoreLayout,
    filter: &dyn RunFilter,
    force: bool,
    progress: &dyn ProgressSink,
) -> CordialResult<Vec<BuildArtifact>> {
    let pairs = discover_active_shadow_pairs(project_root, filter)?;
    let task = progress.bar(
        "Building shadow-dep rustdoc cache".to_string(),
        pairs.len() as u64,
    );
    let mut artifacts = Vec::new();
    for (index, pair) in pairs.iter().enumerate() {
        task.set_message(format!(
            "Building {} via {} ({}/{})",
            pair.upstream(),
            pair.shadow(),
            index + 1,
            pairs.len()
        ));
        match build_shadow_dep_rustdoc_with_progress(
            project_root,
            store,
            pair.shadow(),
            pair.upstream(),
            force,
            progress,
        ) {
            Ok(artifact) => artifacts.push(artifact),
            Err(error) => tracing::warn!(
                upstream = %pair.upstream(),
                shadow = %pair.shadow(),
                %error,
                "skipping shadow-dep rustdoc build"
            ),
        }
        task.inc(1);
    }
    task.finish(format!(
        "Shadow-dep rustdoc cache ready for {} pair(s)",
        artifacts.len()
    ));
    Ok(artifacts)
}

/// Build shadow-dep rustdoc for all active pairs (no crate filter).
#[instrument(level = "debug", skip(store), err(level = "warn"))]
pub fn build_all_active_shadow_deps(
    project_root: &Path,
    store: &StoreLayout,
    force: bool,
) -> CordialResult<Vec<BuildArtifact>> {
    build_active_shadow_deps(project_root, store, &RunAll, force)
}

#[instrument(level = "debug", skip(store, progress), err(level = "warn"))]
pub(crate) fn build_all_active_shadow_deps_with_progress(
    project_root: &Path,
    store: &StoreLayout,
    force: bool,
    progress: &dyn ProgressSink,
) -> CordialResult<Vec<BuildArtifact>> {
    build_active_shadow_deps_with_progress(project_root, store, &RunAll, force, progress)
}
