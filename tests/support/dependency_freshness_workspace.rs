//! Fixture workspaces for the dependency-freshness tests: manifests, lockfile, the
//! survey cache, and the survey CSV.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

pub fn dependency_freshness_survey_csv(store_root: &Path) -> miette::Result<String> {
    fs::read_to_string(
        store_root
            .join("findings")
            .join("dependency-freshness-survey.csv"),
    )
    .into_diagnostic()
    .wrap_err("dependency freshness survey csv")
}

pub fn write_dependency_freshness_cache(store_root: &Path, cache: &str) -> miette::Result<()> {
    let cache_dir = store_root.join("cache");
    fs::create_dir_all(&cache_dir)
        .into_diagnostic()
        .wrap_err("cache dir")?;
    fs::write(cache_dir.join("dependency-freshness.toml"), cache)
        .into_diagnostic()
        .wrap_err("dependency freshness cache")
}

pub fn write_workspace_manifest(root: &Path, manifest: &str) -> miette::Result<()> {
    fs::write(root.join("Cargo.toml"), manifest)
        .into_diagnostic()
        .wrap_err("workspace manifest")
}

pub fn write_member_manifest(root: &Path, rel: &str, manifest: &str) -> miette::Result<()> {
    let crate_root = root.join(rel);
    fs::create_dir_all(crate_root.join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(crate_root.join("Cargo.toml"), manifest)
        .into_diagnostic()
        .wrap_err("member manifest")?;
    fs::write(crate_root.join("src/lib.rs"), "pub fn ok() {}\n")
        .into_diagnostic()
        .wrap_err("lib.rs")
}

pub fn write_package_manifest(
    root: &Path,
    rel: &str,
    package_name: &str,
    version: &str,
) -> miette::Result<()> {
    write_member_manifest(
        root,
        rel,
        &format!(
            r#"
[package]
name = "{package_name}"
version = "{version}"
edition = "2024"
"#
        ),
    )
}

pub fn write_lockfile(root: &Path, lockfile: &str) -> miette::Result<()> {
    fs::write(root.join("Cargo.lock"), lockfile)
        .into_diagnostic()
        .wrap_err("lockfile")
}
