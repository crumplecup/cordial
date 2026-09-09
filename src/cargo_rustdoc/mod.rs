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
use std::time::SystemTime;

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
        targets.retain(|target| target.crate_name() == name);
    }

    let mut artifacts = Vec::new();
    for target in targets {
        let artifact_path = store.build_artifact_path(target.crate_name());
        let cached_json = store.rustdoc_cache_path(target.crate_name());
        if !force
            && artifact_path.is_file()
            && cached_json.is_file()
            && rustdoc_cache_is_fresh(project_root, target.crate_root(), &cached_json)
            && let Ok(existing) = read_build_artifact(&artifact_path)
        {
            artifacts.push(existing);
            continue;
        }

        let json_path = run_cargo_rustdoc(project_root, target.crate_name(), &[])?;

        copy_rustdoc_json(&json_path, &cached_json)?;

        let crate_doc_dir = target.crate_root().join("doc");
        std::fs::create_dir_all(&crate_doc_dir)?;
        let local_json =
            crate_doc_dir.join(format!("{}.json", target.crate_name().replace('-', "_")));
        copy_rustdoc_json(&json_path, &local_json)?;

        let rustdoc_sha256 = hash_file(&cached_json)?;
        let crate_version =
            read_crate_version(&cached_json).unwrap_or_else(|| "unknown".to_string());
        let artifact = BuildArtifact::workspace_member(
            target.crate_name(),
            PathBuf::from(format!("cache/rustdoc/{}.json", target.crate_name())),
            DocFingerprint::new(rustdoc_sha256, crate_version),
        );
        write_build_artifact(&artifact_path, &artifact)?;
        artifacts.push(artifact);
    }

    Ok(artifacts)
}

/// Whether cached rustdoc JSON is newer than the Rust inputs that feed it.
#[instrument(level = "debug", skip(project_root, crate_root, cached_json))]
pub(crate) fn rustdoc_cache_is_fresh(
    project_root: &Path,
    crate_root: &Path,
    cached_json: &Path,
) -> bool {
    let Some(cache_modified) = modified_at(cached_json) else {
        return false;
    };

    !rustdoc_inputs(project_root, crate_root)
        .into_iter()
        .any(|path| modified_at(&path).is_some_and(|modified| modified > cache_modified))
}

#[instrument(level = "trace", skip(project_root, crate_root))]
fn rustdoc_inputs(project_root: &Path, crate_root: &Path) -> Vec<PathBuf> {
    let mut inputs = vec![
        project_root.join("Cargo.toml"),
        project_root.join("Cargo.lock"),
        crate_root.join("Cargo.toml"),
        crate_root.join("build.rs"),
    ];

    let src_root = crate_root.join("src");
    if src_root.is_dir() {
        inputs.extend(
            walkdir::WalkDir::new(src_root)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
                .map(|entry| entry.into_path())
                .filter(|path| path.extension().is_some_and(|extension| extension == "rs")),
        );
    }

    inputs
}

#[instrument(level = "trace", skip(path))]
fn modified_at(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
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

#[cfg(test)]
mod tests {
    use miette::{IntoDiagnostic, WrapErr};

    use super::*;

    #[test]
    fn rustdoc_cache_freshness_tracks_member_sources() -> miette::Result<()> {
        let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
        let project = temp.path();
        let src = project.join("src");
        std::fs::create_dir_all(&src)
            .into_diagnostic()
            .wrap_err("src dir")?;
        let source = src.join("lib.rs");
        let cache = project.join("cache.json");

        std::fs::write(project.join("Cargo.toml"), "[package]\nname = \"demo\"\n")
            .into_diagnostic()
            .wrap_err("manifest")?;
        std::fs::write(&source, "pub struct Before;\n")
            .into_diagnostic()
            .wrap_err("source")?;
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(&cache, "{}\n")
            .into_diagnostic()
            .wrap_err("cache")?;

        assert!(rustdoc_cache_is_fresh(project, project, &cache));

        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(&source, "pub struct After;\n")
            .into_diagnostic()
            .wrap_err("updated source")?;

        assert!(!rustdoc_cache_is_fresh(project, project, &cache));
        Ok(())
    }
}
