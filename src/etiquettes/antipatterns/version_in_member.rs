//! Workspace manifest scan: inline `version` keys in member `Cargo.toml` files.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use toml::Value;
use tracing::instrument;

use crate::error::{CordialError, CordialResult};

use super::types::{AntipatternRuleId, AntipatternSiteRecord};

const DEP_SECTIONS: &[&str] = &["dependencies", "dev-dependencies", "build-dependencies"];

type VersionFindingsByCrate = HashMap<String, Vec<AntipatternSiteRecord>>;
type VersionFindingsCache = Mutex<Option<(PathBuf, VersionFindingsByCrate)>>;

static VERSION_FINDINGS_CACHE: VersionFindingsCache = Mutex::new(None);

/// Cached workspace scan grouped by crate name.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_workspace_version_in_member(
    workspace_root: &Path,
) -> CordialResult<HashMap<String, Vec<AntipatternSiteRecord>>> {
    let cache_key = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());

    if let Ok(cache) = VERSION_FINDINGS_CACHE.lock()
        && let Some((key, findings)) = cache.as_ref()
        && *key == cache_key
    {
        return Ok(findings.clone());
    }

    let meta = match cargo_metadata::MetadataCommand::new()
        .current_dir(workspace_root)
        .exec()
    {
        Ok(meta) => meta,
        Err(_) => return Ok(HashMap::new()),
    };
    if meta.workspace_members.is_empty() {
        return Ok(HashMap::new());
    }

    let root_manifest = workspace_root.join("Cargo.toml");
    let root_content = std::fs::read_to_string(&root_manifest)?;
    let root_table = parse_manifest_table(&root_content, &root_manifest)?;
    let workspace_dep_names = workspace_dependency_names(&root_table);
    let workspace_has_package_version = workspace_package_has_version(&root_table);

    let mut by_crate = HashMap::new();
    for package_id in &meta.workspace_members {
        let package = meta
            .packages
            .iter()
            .find(|candidate| &candidate.id == package_id)
            .ok_or_else(|| {
                CordialError::invariant(format!("missing package metadata for {package_id}"))
            })?;
        let manifest_path = Path::new(package.manifest_path.as_str());
        let crate_root = manifest_path.parent().ok_or_else(|| {
            CordialError::invariant(format!("no parent for {}", manifest_path.display()))
        })?;
        let findings = scan_member_manifest(
            manifest_path,
            crate_root,
            &workspace_dep_names,
            workspace_has_package_version,
        )?;
        if !findings.is_empty() {
            by_crate.insert(package.name.to_string(), findings);
        }
    }

    if let Ok(mut cache) = VERSION_FINDINGS_CACHE.lock() {
        *cache = Some((cache_key, by_crate.clone()));
    }
    Ok(by_crate)
}

/// Scan one member `Cargo.toml` for inline version declarations.
#[instrument(level = "debug", skip(workspace_dep_names), err(level = "warn"))]
pub fn scan_member_manifest(
    manifest_path: &Path,
    crate_root: &Path,
    workspace_dep_names: &HashSet<String>,
    workspace_has_package_version: bool,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let content = std::fs::read_to_string(manifest_path)?;
    let table = parse_manifest_table(&content, manifest_path)?;
    let mut findings = Vec::new();
    let manifest_rel = manifest_rel_path(crate_root, manifest_path);

    if let Some(package) = table.get("package").and_then(Value::as_table)
        && package_uses_inline_version(package)
    {
        let line = find_line(&content, "version").unwrap_or(1);
        findings.push(AntipatternSiteRecord {
            rule_id: AntipatternRuleId::VersionInMember001,
            context: "Cargo.toml [package].version".to_string(),
            file: manifest_rel.clone(),
            line,
            snippet: package_version_snippet(&content, line, workspace_has_package_version),
        });
    }

    for section in DEP_SECTIONS {
        let Some(deps) = table.get(*section).and_then(Value::as_table) else {
            continue;
        };
        scan_dependency_entries(
            deps,
            &format!("[{section}]"),
            &manifest_rel,
            &content,
            workspace_dep_names,
            &mut findings,
        );
    }

    scan_target_dependency_tables(
        &table,
        &manifest_rel,
        &content,
        workspace_dep_names,
        &mut findings,
    );

    findings.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then(left.line.cmp(&right.line))
            .then(left.context.cmp(&right.context))
    });
    Ok(findings)
}

#[instrument(level = "debug", skip(root_table))]
fn workspace_dependency_names(root_table: &toml::Table) -> HashSet<String> {
    root_table
        .get("workspace")
        .and_then(Value::as_table)
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(Value::as_table)
        .map(|deps| deps.keys().cloned().collect())
        .unwrap_or_default()
}

#[instrument(level = "debug", skip(root_table))]
fn workspace_package_has_version(root_table: &toml::Table) -> bool {
    root_table
        .get("workspace")
        .and_then(Value::as_table)
        .and_then(|workspace| workspace.get("package"))
        .and_then(Value::as_table)
        .is_some_and(|package| package.contains_key("version"))
}

#[instrument(level = "debug", skip(package))]
fn package_uses_inline_version(package: &toml::map::Map<String, Value>) -> bool {
    let Some(version) = package.get("version") else {
        return false;
    };
    match version {
        Value::String(_) => true,
        Value::Table(table) => !table
            .get("workspace")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        _ => false,
    }
}

#[instrument(level = "debug", skip(dep, workspace_dep_names))]
fn dependency_should_use_workspace(
    dep_name: &str,
    dep: &Value,
    workspace_dep_names: &HashSet<String>,
) -> bool {
    match dep {
        Value::String(_) => true,
        Value::Table(table) => {
            if table
                .get("workspace")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return false;
            }
            if table.contains_key("git") {
                return false;
            }
            if table.contains_key("path") {
                if table.contains_key("version") {
                    return true;
                }
                return workspace_dep_names.contains(dep_name);
            }
            table.contains_key("version")
        }
        _ => false,
    }
}

#[instrument(level = "debug", skip(deps, workspace_dep_names, findings))]
fn scan_dependency_entries(
    deps: &toml::map::Map<String, Value>,
    section_path: &str,
    manifest_rel: &Path,
    content: &str,
    workspace_dep_names: &HashSet<String>,
    findings: &mut Vec<AntipatternSiteRecord>,
) {
    for (dep_name, dep_value) in deps {
        if !dependency_should_use_workspace(dep_name, dep_value, workspace_dep_names) {
            continue;
        }
        let in_workspace = workspace_dep_names.contains(dep_name.as_str());
        let context = format!("Cargo.toml {section_path} {dep_name}");
        let line = find_dependency_line(content, section_path, dep_name).unwrap_or(1);
        findings.push(AntipatternSiteRecord {
            rule_id: AntipatternRuleId::VersionInMember001,
            context,
            file: manifest_rel.to_path_buf(),
            line,
            snippet: dependency_version_snippet(dep_name, dep_value, in_workspace),
        });
    }
}

#[instrument(level = "debug", skip(table, workspace_dep_names, findings))]
fn scan_target_dependency_tables(
    table: &toml::Table,
    manifest_rel: &Path,
    content: &str,
    workspace_dep_names: &HashSet<String>,
    findings: &mut Vec<AntipatternSiteRecord>,
) {
    let Some(target_root) = table.get("target").and_then(Value::as_table) else {
        return;
    };
    for (cfg_key, cfg_table_value) in target_root {
        let Some(cfg_table) = cfg_table_value.as_table() else {
            continue;
        };
        scan_dependency_sections_in_table(
            cfg_table,
            &format!("[target.{cfg_key}]"),
            manifest_rel,
            content,
            workspace_dep_names,
            findings,
        );
    }
}

#[instrument(level = "debug", skip(table, workspace_dep_names, findings))]
fn scan_dependency_sections_in_table(
    table: &toml::Table,
    section_prefix: &str,
    manifest_rel: &Path,
    content: &str,
    workspace_dep_names: &HashSet<String>,
    findings: &mut Vec<AntipatternSiteRecord>,
) {
    for section in DEP_SECTIONS {
        let Some(deps) = table.get(*section).and_then(Value::as_table) else {
            continue;
        };
        scan_dependency_entries(
            deps,
            &format!("{section_prefix}[{section}]"),
            manifest_rel,
            content,
            workspace_dep_names,
            findings,
        );
    }
    for (key, value) in table {
        if key.starts_with("target")
            && let Some(nested) = value.as_table()
        {
            scan_dependency_sections_in_table(
                nested,
                &format!("{section_prefix}[{key}]"),
                manifest_rel,
                content,
                workspace_dep_names,
                findings,
            );
        }
    }
}

#[instrument(level = "debug")]
fn package_version_snippet(
    content: &str,
    line: u32,
    workspace_has_package_version: bool,
) -> String {
    let current = content
        .lines()
        .nth(line.saturating_sub(1) as usize)
        .map(str::trim)
        .unwrap_or("version = …");
    if workspace_has_package_version {
        format!("{current} → use version.workspace = true")
    } else {
        format!(
            "{current} → add version under root [workspace.package], then use version.workspace = true"
        )
    }
}

#[instrument(level = "debug", skip(dep_value))]
fn dependency_version_snippet(dep_name: &str, dep_value: &Value, in_workspace: bool) -> String {
    let current = match dep_value {
        Value::String(version) => format!("{dep_name} = \"{version}\""),
        Value::Table(table) => {
            if table.contains_key("path") && !table.contains_key("version") {
                format!("{dep_name} = {{ path = \"…\" }}")
            } else {
                let version = table.get("version").and_then(Value::as_str).unwrap_or("…");
                if table.contains_key("features") {
                    format!("{dep_name} = {{ version = \"{version}\", … }}")
                } else {
                    format!("{dep_name} = {{ version = \"{version}\" }}")
                }
            }
        }
        _ => format!("{dep_name} = …"),
    };
    if in_workspace {
        format!("{current} → use {dep_name}.workspace = true")
    } else {
        format!(
            "{current} → add {dep_name} to root [workspace.dependencies], then use {dep_name}.workspace = true"
        )
    }
}

#[instrument(level = "debug")]
fn manifest_rel_path(crate_root: &Path, manifest_path: &Path) -> PathBuf {
    if manifest_path
        .file_name()
        .is_some_and(|name| name == "Cargo.toml")
    {
        PathBuf::from("Cargo.toml")
    } else {
        manifest_path
            .strip_prefix(crate_root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| PathBuf::from("Cargo.toml"))
    }
}

#[instrument(level = "debug")]
fn find_line(content: &str, needle: &str) -> Option<u32> {
    content.lines().enumerate().find_map(|(index, line)| {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            return None;
        }
        trimmed.contains(needle).then_some((index + 1) as u32)
    })
}

#[instrument(level = "debug")]
fn find_dependency_line(content: &str, section_path: &str, dep_name: &str) -> Option<u32> {
    let section_header = if section_path.starts_with('[') {
        section_path.to_string()
    } else {
        format!("[{section_path}]")
    };
    let section_body = section_path.trim_start_matches('[').trim_end_matches(']');
    let nested_header = format!("[{section_body}.{dep_name}]");
    let mut in_section = false;
    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_section =
                trimmed == section_header || trimmed.starts_with(&format!("[{section_body}."));
        }
        if !in_section {
            continue;
        }
        if trimmed == nested_header {
            return Some((index + 1) as u32);
        }
        if trimmed.starts_with(&format!("{dep_name}."))
            || trimmed.starts_with(&format!("{dep_name} ="))
        {
            return Some((index + 1) as u32);
        }
    }
    find_line(content, dep_name)
}

#[instrument(level = "debug", err(level = "warn"))]
fn parse_manifest_table(content: &str, manifest_path: &Path) -> CordialResult<toml::Table> {
    toml::from_str(content).map_err(|error| {
        CordialError::invariant(format!(
            "failed to parse {}: {error}",
            manifest_path.display()
        ))
    })
}
