//! Build upstream rustdoc through shadow-member dependency feature edges.

use std::path::{Path, PathBuf};

use tracing::instrument;

use crate::cargo_rustdoc::{DepBuildConfig, collect_member_dep_build_config};
use crate::error::CordialResult;
use crate::plugin::{discover_active_shadow_pairs, tracked_target_for_shadow};
use crate::session::{RunAll, RunFilter};
use crate::store::StoreLayout;

use super::artifact::BuildArtifact;
use super::cargo::run_cargo_rustdoc;
use super::{
    BuildOptions, copy_rustdoc_json, hash_file, read_build_artifact, read_crate_version,
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
        .filter(|target| target.upstream == upstream_crate)
        .map(|target| DepBuildConfig {
            activated_features: target
                .impl_dep_features
                .iter()
                .map(|feature| (*feature).to_string())
                .collect(),
            uses_default_features: true,
        })
        .unwrap_or_default()
}

/// Build and cache upstream rustdoc for one shadow ↔ upstream pair.
#[instrument(level = "debug", skip(store, options), err(level = "warn"))]
pub fn build_shadow_dep_rustdoc(
    project_root: &Path,
    store: &StoreLayout,
    shadow_crate: &str,
    upstream_crate: &str,
    options: &BuildOptions,
) -> CordialResult<BuildArtifact> {
    store.ensure_dirs()?;
    std::fs::create_dir_all(store.builds_dir())?;
    std::fs::create_dir_all(store.rustdoc_cache_dir())?;

    let artifact_path = store.shadow_dep_build_artifact_path(shadow_crate, upstream_crate);
    let cached_json = store.shadow_dep_rustdoc_cache_path(shadow_crate, upstream_crate);

    if !options.force
        && artifact_path.is_file()
        && cached_json.is_file()
        && let Ok(existing) = read_build_artifact(&artifact_path)
    {
        return Ok(existing);
    }

    let dep_config = resolve_shadow_dep_build_config(project_root, shadow_crate, upstream_crate);
    let feature_refs: Vec<&str> = dep_config
        .activated_features
        .iter()
        .map(String::as_str)
        .collect();
    let json_path = run_cargo_rustdoc(project_root, upstream_crate, &feature_refs)?;
    copy_rustdoc_json(&json_path, &cached_json)?;

    let rustdoc_sha256 = hash_file(&cached_json)?;
    let crate_version = read_crate_version(&cached_json).unwrap_or_else(|| "unknown".to_string());
    let relative_json = PathBuf::from(format!(
        "cache/rustdoc/{}.json",
        StoreLayout::shadow_dep_cache_stem(shadow_crate, upstream_crate)
    ));
    let mut artifact = BuildArtifact::shadow_dep(
        shadow_crate,
        upstream_crate,
        relative_json,
        dep_config.activated_features,
        dep_config.uses_default_features,
    );
    artifact.fingerprint = Some(super::artifact::DocFingerprint {
        rustdoc_sha256,
        crate_version,
    });
    write_build_artifact(&artifact_path, &artifact)?;
    Ok(artifact)
}

/// Build shadow-dep rustdoc for every active tracked pair in the workspace.
#[instrument(level = "debug", skip(store, filter, options), err(level = "warn"))]
pub fn build_active_shadow_deps(
    project_root: &Path,
    store: &StoreLayout,
    filter: &dyn RunFilter,
    options: &BuildOptions,
) -> CordialResult<Vec<BuildArtifact>> {
    let pairs = discover_active_shadow_pairs(project_root, filter)?;
    let mut artifacts = Vec::new();
    for pair in pairs {
        match build_shadow_dep_rustdoc(project_root, store, &pair.shadow, &pair.upstream, options) {
            Ok(artifact) => artifacts.push(artifact),
            Err(error) => tracing::warn!(
                upstream = %pair.upstream,
                shadow = %pair.shadow,
                %error,
                "skipping shadow-dep rustdoc build"
            ),
        }
    }
    Ok(artifacts)
}

/// Build shadow-dep rustdoc for all active pairs (no crate filter).
#[instrument(level = "debug", skip(store, options), err(level = "warn"))]
pub fn build_all_active_shadow_deps(
    project_root: &Path,
    store: &StoreLayout,
    options: &BuildOptions,
) -> CordialResult<Vec<BuildArtifact>> {
    build_active_shadow_deps(project_root, store, &RunAll, options)
}
