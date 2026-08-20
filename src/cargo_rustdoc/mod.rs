//! Build rustdoc JSON for workspace members and cache artifacts under the store.

mod artifact;
mod cargo;
#[cfg(any(feature = "impl_coverage", feature = "shadow"))]
mod dep_features;
#[cfg(feature = "shadow")]
mod shadow_dep;
#[cfg(feature = "homecoming_std")]
mod sysroot;

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tracing::instrument;

pub use artifact::{BuildArtifact, BuildKind, DocFingerprint};
pub use cargo::{nightly_available, run_cargo_rustdoc};
#[cfg(any(feature = "impl_coverage", feature = "shadow"))]
pub use dep_features::{
    DepBuildConfig, collect_dep_serde_features, collect_member_dep_build_config,
};
#[cfg(feature = "shadow")]
pub use shadow_dep::{
    build_active_shadow_deps, build_all_active_shadow_deps, build_shadow_dep_rustdoc,
    resolve_shadow_dep_build_config,
};
#[cfg(feature = "homecoming_std")]
pub use sysroot::{build_sysroot_libraries, is_std_family_crate, resolve_sysroot_library_manifest};

use crate::error::CordialResult;
use crate::session::RunAll;
use crate::store::StoreLayout;
use crate::targets::discover_crate_targets;

/// Build rustdoc JSON for workspace members and write elicit_doc-compatible cache artifacts.
#[instrument(level = "debug", skip(store), err(level = "warn"))]
pub fn build_workspace_members(
    project_root: &Path,
    store: &StoreLayout,
    only_crate: Option<&str>,
    force: bool,
) -> CordialResult<Vec<BuildArtifact>> {
    store.ensure_dirs()?;
    std::fs::create_dir_all(store.builds_dir())?;
    std::fs::create_dir_all(store.rustdoc_cache_dir())?;

    let filter = RunAll;
    let mut targets = discover_crate_targets(project_root, &filter)?;
    if let Some(name) = only_crate {
        targets.retain(|target| target.crate_name == name);
    }

    let mut artifacts = Vec::new();
    for target in targets {
        let artifact_path = store.build_artifact_path(&target.crate_name);
        if !force
            && artifact_path.is_file()
            && let Ok(existing) = read_build_artifact(&artifact_path)
        {
            let cached_json = store.rustdoc_cache_path(&target.crate_name);
            if cached_json.is_file() {
                artifacts.push(existing);
                continue;
            }
        }

        let json_path = run_cargo_rustdoc(project_root, &target.crate_name, &[])?;

        let cached_json = store.rustdoc_cache_path(&target.crate_name);
        copy_rustdoc_json(&json_path, &cached_json)?;

        let crate_doc_dir = target.crate_root.join("doc");
        std::fs::create_dir_all(&crate_doc_dir)?;
        let local_json =
            crate_doc_dir.join(format!("{}.json", target.crate_name.replace('-', "_")));
        copy_rustdoc_json(&json_path, &local_json)?;

        let rustdoc_sha256 = hash_file(&cached_json)?;
        let crate_version =
            read_crate_version(&cached_json).unwrap_or_else(|| "unknown".to_string());
        let artifact = BuildArtifact::workspace_member(
            &target.crate_name,
            PathBuf::from(format!("cache/rustdoc/{}.json", target.crate_name)),
            DocFingerprint::new(rustdoc_sha256, crate_version),
        );
        write_build_artifact(&artifact_path, &artifact)?;
        artifacts.push(artifact);
    }

    Ok(artifacts)
}

#[instrument(level = "debug", err(level = "warn"))]
pub(crate) fn copy_rustdoc_json(from: &Path, to: &Path) -> CordialResult<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(from, to)?;
    Ok(())
}

#[instrument(level = "debug", skip(path), err(level = "warn"))]
pub(crate) fn hash_file(path: &Path) -> CordialResult<String> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

#[instrument(level = "info")]
pub(crate) fn read_crate_version(json_path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(json_path).ok()?;
    let krate: rustdoc_types::Crate = serde_json::from_str(&content).ok()?;
    krate.crate_version
}

#[instrument(level = "info", skip(path), err(level = "warn"))]
pub(crate) fn read_build_artifact(path: &Path) -> CordialResult<BuildArtifact> {
    let bytes = std::fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

#[instrument(level = "info", skip(path, artifact), err(level = "warn"))]
pub(crate) fn write_build_artifact(path: &Path, artifact: &BuildArtifact) -> CordialResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(artifact)?)?;
    Ok(())
}
