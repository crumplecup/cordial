use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::error::{CordialError, CordialResult};

use crate::etiquettes::dependency_freshness::types::{
    DependencyFreshnessObservation, DependencySourceKind, DependencySurveyRecord,
};

use tracing::instrument;

type FreshnessIndex = BTreeMap<String, Vec<String>>;

/// Optional file under `{store}/cache/` overriding Cargo freshness observations.
const DEPENDENCY_FRESHNESS_CACHE_FILE: &str = "dependency-freshness.toml";

/// Path of the optional deterministic freshness override.
#[instrument(level = "debug")]
fn dependency_freshness_cache_path(store_root: &Path) -> PathBuf {
    store_root
        .join("cache")
        .join(DEPENDENCY_FRESHNESS_CACHE_FILE)
}

#[instrument(level = "debug")]
fn freshness_cache_index(store_root: &Path) -> CordialResult<FreshnessIndex> {
    let path = dependency_freshness_cache_path(store_root);
    if !path.is_file() {
        return Ok(BTreeMap::new());
    }
    let content = fs::read_to_string(&path)?;
    let cache: DependencyFreshnessCache = toml::from_str(&content).map_err(|error| {
        CordialError::invariant(format!("failed to parse {}: {error}", path.display()))
    })?;
    let mut out: FreshnessIndex = BTreeMap::new();
    for package in cache.package {
        out.entry(package.name)
            .or_default()
            .push(package.available_version);
    }
    for versions in out.values_mut() {
        versions.sort();
        versions.dedup();
    }
    Ok(out)
}

#[instrument(level = "debug")]
pub(super) fn freshness_index(
    project_root: &Path,
    store_root: Option<&Path>,
) -> CordialResult<FreshnessIndex> {
    if let Some(store_root) = store_root {
        let cache_path = dependency_freshness_cache_path(store_root);
        if cache_path.is_file() {
            return freshness_cache_index(store_root);
        }
    }
    cargo_update_dry_run_freshness_index(project_root)
}

#[instrument(level = "debug")]
fn cargo_update_dry_run_freshness_index(project_root: &Path) -> CordialResult<FreshnessIndex> {
    let output = Command::new("cargo")
        .current_dir(project_root)
        .arg("update")
        .arg("--dry-run")
        .arg("--verbose")
        .output()?;
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    if !output.stderr.is_empty() {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    if !output.status.success() {
        return Err(CordialError::invariant(format!(
            "`cargo update --dry-run --verbose` failed while checking dependency freshness: {text}"
        )));
    }
    Ok(parse_cargo_update_dry_run_freshness(&text))
}

#[instrument(level = "debug")]
fn parse_cargo_update_dry_run_freshness(output: &str) -> FreshnessIndex {
    let mut out: FreshnessIndex = BTreeMap::new();
    for line in output.lines() {
        let Some((name, available_version)) = parse_cargo_update_line(line) else {
            continue;
        };
        out.entry(name).or_default().push(available_version);
    }
    for versions in out.values_mut() {
        versions.sort();
        versions.dedup();
    }
    out
}

#[instrument(level = "debug")]
fn parse_cargo_update_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    parse_cargo_updating_line(trimmed).or_else(|| parse_cargo_unchanged_line(trimmed))
}

#[instrument(level = "debug")]
fn parse_cargo_updating_line(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("Updating ")?;
    let (left, right) = rest.split_once(" -> ")?;
    let (name, _) = left.rsplit_once(" v")?;
    let version = right.split_whitespace().next()?.strip_prefix('v')?;
    Some((name.to_string(), version.to_string()))
}

#[instrument(level = "debug")]
fn parse_cargo_unchanged_line(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("Unchanged ")?;
    let (left, right) = rest.split_once(" (available: ")?;
    let (name, _) = left.rsplit_once(" v")?;
    let version = right.strip_suffix(')')?.strip_prefix('v')?;
    Some((name.to_string(), version.to_string()))
}

#[instrument(level = "debug", skip(record, freshness))]
pub(super) fn add_freshness_observations(
    record: &mut DependencySurveyRecord,
    freshness: &FreshnessIndex,
) {
    if !record_can_have_freshness_observations(record) {
        return;
    }
    let Some(available_versions) = freshness.get(record.package_name()) else {
        return;
    };
    for locked_version in record.locked_versions().clone() {
        for available_version in available_versions {
            if let Some(observation) = DependencyFreshnessObservation::from_versions(
                record.package_name().clone(),
                locked_version.clone(),
                available_version.clone(),
            ) {
                record.add_freshness_observation(observation);
            }
        }
    }
}

#[instrument(level = "debug", skip(record))]
pub(super) fn record_can_have_freshness_observations(record: &DependencySurveyRecord) -> bool {
    matches!(
        record.source_kind(),
        DependencySourceKind::Registry | DependencySourceKind::Workspace
    )
}

#[derive(Debug, Default, Deserialize)]
struct DependencyFreshnessCache {
    #[serde(default)]
    package: Vec<DependencyFreshnessCachePackage>,
}

#[derive(Debug, Deserialize)]
struct DependencyFreshnessCachePackage {
    name: String,
    available_version: String,
}
