use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::error::{CordialError, CordialResult};

use super::types::{
    DependencyFreshnessIndicator, DependencyFreshnessObservation, DependencySection,
    DependencySourceKind, DependencySurveyRecord, DependencySurveyRecordInput, ManifestVersionSpec,
};

type LockfileIndex = BTreeMap<String, Vec<String>>;
type FreshnessIndex = BTreeMap<String, Vec<String>>;

/// Optional file under `{store}/cache/` overriding Cargo freshness observations.
pub const DEPENDENCY_FRESHNESS_CACHE_FILE: &str = "dependency-freshness.toml";

#[derive(Debug, Clone)]
struct DependencyEntry {
    dependency_name: String,
    package_name: String,
    section: DependencySection,
    version_spec: ManifestVersionSpec,
    source_kind: DependencySourceKind,
    line: u32,
    indicators: Vec<DependencyFreshnessIndicator>,
}

/// Survey one crate and join Cargo registry freshness facts.
pub(crate) fn survey_crate_dependency_freshness(
    project_root: &Path,
    crate_root: &Path,
    crate_name: &str,
    store_root: Option<&Path>,
) -> CordialResult<Vec<DependencySurveyRecord>> {
    let manifest_path = crate_root.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path)?;
    let manifest_table = parse_table(&manifest, &manifest_path)?;
    let workspace_dependencies = workspace_dependencies(project_root)?;
    let lockfile_index = lockfile_index(project_root)?;
    let mut records = Vec::new();

    for entry in dependency_entries(&manifest, &manifest_table, &workspace_dependencies) {
        let locked_versions = locked_versions_for_entry(&entry, &lockfile_index);
        let record = DependencySurveyRecord::from_input(DependencySurveyRecordInput {
            crate_name: crate_name.to_string(),
            dependency_name: entry.dependency_name,
            package_name: entry.package_name.clone(),
            manifest_path: manifest_path.clone(),
            line: entry.line,
            section: entry.section,
            version_spec: entry.version_spec,
            source_kind: entry.source_kind,
            locked_versions,
            indicators: entry.indicators,
        });
        records.push(record);
    }
    if records.iter().any(record_can_have_freshness_observations) {
        let freshness_index = freshness_index(project_root, store_root)?;
        for record in &mut records {
            add_freshness_observations(record, &freshness_index);
        }
    }

    Ok(records)
}

/// Path of the optional deterministic freshness override.
fn dependency_freshness_cache_path(store_root: &Path) -> PathBuf {
    store_root
        .join("cache")
        .join(DEPENDENCY_FRESHNESS_CACHE_FILE)
}

fn workspace_dependencies(project_root: &Path) -> CordialResult<BTreeMap<String, DependencyEntry>> {
    let manifest_path = project_root.join("Cargo.toml");
    if !manifest_path.is_file() {
        return Ok(BTreeMap::new());
    }
    let manifest = fs::read_to_string(&manifest_path)?;
    let table = parse_table(&manifest, &manifest_path)?;
    let Some(workspace) = table.get("workspace").and_then(toml::Value::as_table) else {
        return Ok(BTreeMap::new());
    };
    let Some(dependencies) = workspace
        .get("dependencies")
        .and_then(toml::Value::as_table)
    else {
        return Ok(BTreeMap::new());
    };
    let entries = parse_dependency_table(
        &manifest,
        DependencySection::Workspace,
        dependencies,
        &BTreeMap::new(),
    );
    Ok(entries
        .into_iter()
        .map(|entry| (entry.dependency_name.clone(), entry))
        .collect())
}

fn dependency_entries(
    manifest: &str,
    table: &toml::Table,
    workspace_dependencies: &BTreeMap<String, DependencyEntry>,
) -> Vec<DependencyEntry> {
    let mut entries = Vec::new();
    collect_root_dependency_table(
        manifest,
        table,
        "dependencies",
        DependencySection::Normal,
        workspace_dependencies,
        &mut entries,
    );
    collect_root_dependency_table(
        manifest,
        table,
        "dev-dependencies",
        DependencySection::Dev,
        workspace_dependencies,
        &mut entries,
    );
    collect_root_dependency_table(
        manifest,
        table,
        "build-dependencies",
        DependencySection::Build,
        workspace_dependencies,
        &mut entries,
    );
    collect_target_dependency_tables(manifest, table, workspace_dependencies, &mut entries);
    entries.sort_by(|left, right| {
        left.section
            .cmp(&right.section)
            .then_with(|| left.dependency_name.cmp(&right.dependency_name))
    });
    entries
}

fn collect_root_dependency_table(
    manifest: &str,
    table: &toml::Table,
    key: &str,
    section: DependencySection,
    workspace_dependencies: &BTreeMap<String, DependencyEntry>,
    entries: &mut Vec<DependencyEntry>,
) {
    if let Some(dependencies) = table.get(key).and_then(toml::Value::as_table) {
        entries.extend(parse_dependency_table(
            manifest,
            section,
            dependencies,
            workspace_dependencies,
        ));
    }
}

fn collect_target_dependency_tables(
    manifest: &str,
    table: &toml::Table,
    workspace_dependencies: &BTreeMap<String, DependencyEntry>,
    entries: &mut Vec<DependencyEntry>,
) {
    let Some(targets) = table.get("target").and_then(toml::Value::as_table) else {
        return;
    };
    for (target, target_table) in targets {
        let Some(target_table) = target_table.as_table() else {
            continue;
        };
        collect_target_dependency_table(
            manifest,
            target_table,
            "dependencies",
            DependencySection::TargetNormal(target.clone()),
            workspace_dependencies,
            entries,
        );
        collect_target_dependency_table(
            manifest,
            target_table,
            "dev-dependencies",
            DependencySection::TargetDev(target.clone()),
            workspace_dependencies,
            entries,
        );
        collect_target_dependency_table(
            manifest,
            target_table,
            "build-dependencies",
            DependencySection::TargetBuild(target.clone()),
            workspace_dependencies,
            entries,
        );
    }
}

fn collect_target_dependency_table(
    manifest: &str,
    target_table: &toml::map::Map<String, toml::Value>,
    dependency_key: &str,
    section: DependencySection,
    workspace_dependencies: &BTreeMap<String, DependencyEntry>,
    entries: &mut Vec<DependencyEntry>,
) {
    let Some(dependencies) = target_table
        .get(dependency_key)
        .and_then(toml::Value::as_table)
    else {
        return;
    };
    entries.extend(parse_dependency_table(
        manifest,
        section,
        dependencies,
        workspace_dependencies,
    ));
}

fn parse_dependency_table(
    manifest: &str,
    section: DependencySection,
    dependencies: &toml::map::Map<String, toml::Value>,
    workspace_dependencies: &BTreeMap<String, DependencyEntry>,
) -> Vec<DependencyEntry> {
    dependencies
        .iter()
        .map(|(dependency_name, declaration)| {
            parse_dependency_entry(
                manifest,
                section.clone(),
                dependency_name,
                declaration,
                workspace_dependencies,
            )
        })
        .collect()
}

fn parse_dependency_entry(
    manifest: &str,
    section: DependencySection,
    dependency_name: &str,
    declaration: &toml::Value,
    workspace_dependencies: &BTreeMap<String, DependencyEntry>,
) -> DependencyEntry {
    let line = dependency_line(manifest, &section, dependency_name).unwrap_or(1);
    match declaration {
        toml::Value::String(requirement) => DependencyEntry {
            dependency_name: dependency_name.to_string(),
            package_name: dependency_name.to_string(),
            section,
            version_spec: ManifestVersionSpec::Requirement(requirement.clone()),
            source_kind: DependencySourceKind::Registry,
            line,
            indicators: workspace_bypass_indicators(workspace_dependencies, dependency_name, false),
        },
        toml::Value::Table(table) => {
            let workspace_inherited = table
                .get("workspace")
                .and_then(toml::Value::as_bool)
                .unwrap_or(false);
            let workspace_entry = workspace_dependencies.get(dependency_name);
            let package_name = table
                .get("package")
                .and_then(toml::Value::as_str)
                .map(str::to_string)
                .or_else(|| workspace_entry.map(|entry| entry.package_name.clone()))
                .unwrap_or_else(|| dependency_name.to_string());
            let inherited_requirement =
                workspace_entry.and_then(|entry| match entry.version_spec.clone() {
                    ManifestVersionSpec::Requirement(requirement) => Some(requirement),
                    ManifestVersionSpec::WorkspaceInherited(requirement) => requirement,
                    ManifestVersionSpec::Unspecified => None,
                });
            let version_spec = if workspace_inherited {
                ManifestVersionSpec::WorkspaceInherited(inherited_requirement)
            } else if let Some(requirement) = table.get("version").and_then(toml::Value::as_str) {
                ManifestVersionSpec::Requirement(requirement.to_string())
            } else if let Some(workspace_entry) = workspace_entry {
                workspace_entry.version_spec.clone()
            } else {
                ManifestVersionSpec::Unspecified
            };
            DependencyEntry {
                dependency_name: dependency_name.to_string(),
                package_name,
                section,
                version_spec,
                source_kind: source_kind(table, workspace_inherited),
                line,
                indicators: workspace_bypass_indicators(
                    workspace_dependencies,
                    dependency_name,
                    workspace_inherited,
                ),
            }
        }
        _ => DependencyEntry {
            dependency_name: dependency_name.to_string(),
            package_name: dependency_name.to_string(),
            section,
            version_spec: ManifestVersionSpec::Unspecified,
            source_kind: DependencySourceKind::Unspecified,
            line,
            indicators: workspace_bypass_indicators(workspace_dependencies, dependency_name, false),
        },
    }
}

fn workspace_bypass_indicators(
    workspace_dependencies: &BTreeMap<String, DependencyEntry>,
    dependency_name: &str,
    workspace_inherited: bool,
) -> Vec<DependencyFreshnessIndicator> {
    if workspace_inherited || !workspace_dependencies.contains_key(dependency_name) {
        Vec::new()
    } else {
        vec![DependencyFreshnessIndicator::ManifestWorkspaceBypass]
    }
}

fn source_kind(
    table: &toml::map::Map<String, toml::Value>,
    workspace_inherited: bool,
) -> DependencySourceKind {
    if workspace_inherited {
        DependencySourceKind::Workspace
    } else if table.contains_key("path") {
        DependencySourceKind::Path
    } else if table.contains_key("git") {
        DependencySourceKind::Git
    } else if table.contains_key("registry") || table.contains_key("version") {
        DependencySourceKind::Registry
    } else {
        DependencySourceKind::Unspecified
    }
}

fn lockfile_index(project_root: &Path) -> CordialResult<LockfileIndex> {
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

fn locked_versions_for_entry(
    entry: &DependencyEntry,
    lockfile_index: &LockfileIndex,
) -> Vec<String> {
    let locked_versions = lockfile_index
        .get(&entry.package_name)
        .cloned()
        .unwrap_or_default();
    let Some(requirement) = version_requirement(&entry.version_spec) else {
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

fn version_requirement(version_spec: &ManifestVersionSpec) -> Option<&str> {
    match version_spec {
        ManifestVersionSpec::Requirement(requirement)
        | ManifestVersionSpec::WorkspaceInherited(Some(requirement)) => Some(requirement),
        ManifestVersionSpec::WorkspaceInherited(None) | ManifestVersionSpec::Unspecified => None,
    }
}

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

fn freshness_index(
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

fn parse_cargo_update_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    parse_cargo_updating_line(trimmed).or_else(|| parse_cargo_unchanged_line(trimmed))
}

fn parse_cargo_updating_line(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("Updating ")?;
    let (left, right) = rest.split_once(" -> ")?;
    let (name, _) = left.rsplit_once(" v")?;
    let version = right.split_whitespace().next()?.strip_prefix('v')?;
    Some((name.to_string(), version.to_string()))
}

fn parse_cargo_unchanged_line(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("Unchanged ")?;
    let (left, right) = rest.split_once(" (available: ")?;
    let (name, _) = left.rsplit_once(" v")?;
    let version = right.strip_suffix(')')?.strip_prefix('v')?;
    Some((name.to_string(), version.to_string()))
}

fn add_freshness_observations(record: &mut DependencySurveyRecord, freshness: &FreshnessIndex) {
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

fn record_can_have_freshness_observations(record: &DependencySurveyRecord) -> bool {
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

fn parse_table(content: &str, path: &Path) -> CordialResult<toml::Table> {
    toml::from_str(content).map_err(|error| {
        CordialError::invariant(format!("failed to parse {}: {error}", path.display()))
    })
}

fn dependency_line(
    manifest: &str,
    section: &DependencySection,
    dependency_name: &str,
) -> Option<u32> {
    let mut active = false;
    let section_headers = section_headers(section);
    for (index, line) in manifest.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            active = section_headers.iter().any(|header| trimmed == *header);
            continue;
        }
        if active
            && (trimmed.starts_with(&format!("{dependency_name} ="))
                || trimmed.starts_with(&format!("{dependency_name}.")))
        {
            return Some((index + 1) as u32);
        }
    }
    manifest
        .lines()
        .enumerate()
        .find(|(_, line)| {
            line.trim_start()
                .starts_with(&format!("{dependency_name} ="))
        })
        .map(|(index, _)| (index + 1) as u32)
}

fn section_headers(section: &DependencySection) -> Vec<String> {
    match section {
        DependencySection::Normal => vec!["[dependencies]".to_string()],
        DependencySection::Dev => vec!["[dev-dependencies]".to_string()],
        DependencySection::Build => vec!["[build-dependencies]".to_string()],
        DependencySection::Workspace => vec!["[workspace.dependencies]".to_string()],
        DependencySection::TargetNormal(target) => target_headers(target, "dependencies"),
        DependencySection::TargetDev(target) => target_headers(target, "dev-dependencies"),
        DependencySection::TargetBuild(target) => target_headers(target, "build-dependencies"),
    }
}

fn target_headers(target: &str, section: &str) -> Vec<String> {
    vec![
        format!("[target.'{target}'.{section}]"),
        format!("[target.\"{target}\".{section}]"),
    ]
}
