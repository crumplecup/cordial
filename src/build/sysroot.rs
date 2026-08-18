//! Build rustdoc JSON for std-family sysroot libraries (`std`, `core`, `alloc`).

use std::path::PathBuf;

use tracing::instrument;

use super::artifact::{BuildArtifact, DocFingerprint};
use super::cargo::{nightly_cargo, nightly_host_target, resolve_nightly_cargo_binary};
use super::{
    BuildOptions, copy_rustdoc_json, hash_file, read_build_artifact, read_crate_version,
    write_build_artifact,
};
use crate::error::{CordialError, CordialResult};
use crate::framework_std::FRAMEWORK_STD_SOURCES;
use crate::store::SysrootCache;

/// Whether `crate_name` is a std-family library documented from the sysroot.
#[instrument(level = "trace", ret)]
pub fn is_std_family_crate(crate_name: &str) -> bool {
    FRAMEWORK_STD_SOURCES.contains(&crate_name)
}

/// Path to `{toolchain}/lib/rustlib/src/rust/library/{crate}/Cargo.toml`.
#[instrument(level = "debug", err(level = "warn"))]
pub fn resolve_sysroot_library_manifest(crate_name: &str) -> CordialResult<PathBuf> {
    let cargo = resolve_nightly_cargo_binary().ok_or_else(|| {
        CordialError::invariant(
            "nightly toolchain required for std-family rustdoc JSON (install via rustup)",
        )
    })?;
    let toolchain_root = cargo
        .parent()
        .and_then(|bin_dir| bin_dir.parent())
        .ok_or_else(|| {
            CordialError::invariant(
                "could not resolve nightly toolchain root from cargo binary path",
            )
        })?;
    let manifest = toolchain_root
        .join("lib/rustlib/src/rust/library")
        .join(crate_name)
        .join("Cargo.toml");
    if !manifest.is_file() {
        return Err(CordialError::invariant(format!(
            "sysroot manifest for `{crate_name}` not found at {}",
            manifest.display()
        )));
    }
    Ok(manifest)
}

/// Build rustdoc JSON for std-family sysroot libraries and cache under [`SysrootCache`].
#[instrument(level = "debug", skip(options), err(level = "warn"))]
pub fn build_sysroot_libraries(
    sysroot: &SysrootCache,
    only_crate: Option<&str>,
    options: &BuildOptions,
) -> CordialResult<Vec<BuildArtifact>> {
    sysroot.ensure_dirs()?;

    let mut sources: Vec<&str> = FRAMEWORK_STD_SOURCES.to_vec();
    if let Some(name) = only_crate {
        if !is_std_family_crate(name) {
            return Err(CordialError::invariant(format!(
                "sysroot build requested for non-std-family crate `{name}`"
            )));
        }
        sources.retain(|source| *source == name);
    }

    let mut artifacts = Vec::new();
    for crate_name in sources {
        let artifact_path = sysroot.build_artifact_path(crate_name);
        if !options.force
            && artifact_path.is_file()
            && let Ok(existing) = read_build_artifact(&artifact_path)
        {
            let cached_json = sysroot.rustdoc_cache_path(crate_name);
            if cached_json.is_file() {
                artifacts.push(existing);
                continue;
            }
        }

        let json_path = run_sysroot_rustdoc(sysroot, crate_name)?;
        let cached_json = sysroot.rustdoc_cache_path(crate_name);
        copy_rustdoc_json(&json_path, &cached_json)?;

        let rustdoc_sha256 = hash_file(&cached_json)?;
        let crate_version =
            read_crate_version(&cached_json).unwrap_or_else(|| "unknown".to_string());
        let mut artifact = BuildArtifact::sysroot_library(
            crate_name,
            PathBuf::from(format!("cache/rustdoc/{crate_name}.json")),
        );
        artifact.fingerprint = Some(DocFingerprint {
            rustdoc_sha256,
            crate_version,
        });
        write_build_artifact(&artifact_path, &artifact)?;
        artifacts.push(artifact);
    }

    Ok(artifacts)
}

#[instrument(skip(sysroot), fields(crate_name))]
fn run_sysroot_rustdoc(sysroot: &SysrootCache, crate_name: &str) -> CordialResult<PathBuf> {
    let manifest = resolve_sysroot_library_manifest(crate_name)?;
    let host_target = nightly_host_target()?;
    let target_dir = sysroot.build_target_dir();

    let mut cmd = nightly_cargo();
    cmd.arg("rustdoc")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--target")
        .arg(&host_target)
        .arg("--target-dir")
        .arg(&target_dir)
        .arg("--")
        .arg("--output-format")
        .arg("json")
        .arg("-Z")
        .arg("unstable-options");

    tracing::debug!(
        manifest = %manifest.display(),
        host_target,
        target_dir = %target_dir.display(),
        "running sysroot cargo rustdoc"
    );
    let status = cmd.status().map_err(CordialError::from)?;
    if !status.success() {
        return Err(CordialError::invariant(format!(
            "sysroot cargo rustdoc for {crate_name} exited with {status}"
        )));
    }

    let normalized = crate_name.replace('-', "_");
    let json_path = target_dir
        .join(&host_target)
        .join("doc")
        .join(format!("{normalized}.json"));
    if !json_path.is_file() {
        return Err(CordialError::invariant(format!(
            "rustdoc JSON not found at {}",
            json_path.display()
        )));
    }

    tracing::debug!(path = %json_path.display(), "sysroot rustdoc JSON produced");
    Ok(json_path)
}

#[cfg(test)]
mod tests {
    use miette::{IntoDiagnostic, WrapErr};

    use super::*;

    #[test]
    fn is_std_family_crate_recognizes_std_core_alloc() {
        assert!(is_std_family_crate("std"));
        assert!(is_std_family_crate("core"));
        assert!(is_std_family_crate("alloc"));
        assert!(!is_std_family_crate("homecoming_core"));
    }

    #[test]
    fn resolve_sysroot_library_manifest_finds_std_when_nightly_installed() -> miette::Result<()> {
        if resolve_nightly_cargo_binary().is_none() {
            return Ok(());
        }
        let manifest = resolve_sysroot_library_manifest("std")
            .into_diagnostic()
            .wrap_err("std manifest")?;
        assert!(manifest.ends_with("library/std/Cargo.toml"));
        assert!(manifest.is_file());
        Ok(())
    }
}
