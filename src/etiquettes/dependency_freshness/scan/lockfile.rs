use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::error::CordialResult;

use super::entry::DependencyEntry;
use super::parse_table;
use crate::etiquettes::dependency_freshness::types::ManifestVersionSpec;

use tracing::instrument;

pub(super) type LockfileIndex = BTreeMap<String, Vec<String>>;

#[instrument(level = "debug")]
pub(super) fn lockfile_index(project_root: &Path) -> CordialResult<LockfileIndex> {
    let lockfile_path = project_root.join("Cargo.lock");
    if !lockfile_path.is_file() {
        return Ok(BTreeMap::new());
    }
    let lockfile = fs::read_to_string(&lockfile_path)?;
    let table = parse_table(&lockfile, &lockfile_path)?;
    let mut out: LockfileIndex = BTreeMap::new();
    let Some(packages) = table.get("package").and_then(toml::Value::as_array) else {
        return Ok(out);
    };
    for package in packages {
        let Some(package) = package.as_table() else {
            continue;
        };
        let Some(name) = package.get("name").and_then(toml::Value::as_str) else {
            continue;
        };
        let Some(version) = package.get("version").and_then(toml::Value::as_str) else {
            continue;
        };
        out.entry(name.to_string())
            .or_default()
            .push(version.to_string());
    }
    for versions in out.values_mut() {
        versions.sort();
        versions.dedup();
    }
    Ok(out)
}

#[instrument(level = "debug", skip(entry, lockfile_index))]
pub(super) fn locked_versions_for_entry(
    entry: &DependencyEntry,
    lockfile_index: &LockfileIndex,
) -> Vec<String> {
    let locked_versions = lockfile_index
        .get(entry.package_name())
        .cloned()
        .unwrap_or_default();
    let Some(requirement) = version_requirement(entry.version_spec()) else {
        return locked_versions;
    };
    let Ok(requirement) = semver::VersionReq::parse(requirement) else {
        return locked_versions;
    };
    let filtered = locked_versions
        .iter()
        .filter(|version| {
            semver::Version::parse(version)
                .map(|version| requirement.matches(&version))
                .unwrap_or(true)
        })
        .cloned()
        .collect::<Vec<_>>();
    if filtered.is_empty() {
        locked_versions
    } else {
        filtered
    }
}

#[instrument(level = "debug", skip(version_spec))]
fn version_requirement(version_spec: &ManifestVersionSpec) -> Option<&str> {
    match version_spec {
        ManifestVersionSpec::Requirement(requirement)
        | ManifestVersionSpec::WorkspaceInherited(Some(requirement)) => Some(requirement),
        ManifestVersionSpec::WorkspaceInherited(None) | ManifestVersionSpec::Unspecified => None,
    }
}
