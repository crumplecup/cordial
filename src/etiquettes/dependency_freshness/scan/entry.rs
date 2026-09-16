use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::error::CordialResult;

use super::parse_table;
use crate::etiquettes::dependency_freshness::types::{
    DependencyFreshnessIndicator, DependencySection, DependencySourceKind, ManifestVersionSpec,
};

use tracing::instrument;

#[derive(Debug, Clone, derive_getters::Getters)]
pub(super) struct DependencyEntry {
    dependency_name: String,
    package_name: String,
    section: DependencySection,
    version_spec: ManifestVersionSpec,
    source_kind: DependencySourceKind,
    #[getter(copy)]
    line: u32,
    indicators: Vec<DependencyFreshnessIndicator>,
}

#[instrument(level = "debug")]
pub(super) fn workspace_dependencies(
    project_root: &Path,
) -> CordialResult<BTreeMap<String, DependencyEntry>> {
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

#[instrument(level = "debug", skip(manifest, table, workspace_dependencies))]
pub(super) fn dependency_entries(
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

#[instrument(
    level = "debug",
    skip(manifest, table, section, workspace_dependencies, entries)
)]
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

#[instrument(
    level = "debug",
    skip(manifest, table, workspace_dependencies, entries)
)]
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

#[instrument(
    level = "debug",
    skip(manifest, target_table, section, workspace_dependencies, entries)
)]
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

#[instrument(
    level = "debug",
    skip(manifest, section, dependencies, workspace_dependencies)
)]
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

#[instrument(
    level = "debug",
    skip(manifest, section, declaration, workspace_dependencies)
)]
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
        toml::Value::Table(table) => table_dependency_entry(
            section,
            dependency_name,
            table,
            workspace_dependencies,
            line,
        ),
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

#[instrument(level = "debug", skip(section, table, workspace_dependencies))]
fn table_dependency_entry(
    section: DependencySection,
    dependency_name: &str,
    table: &toml::map::Map<String, toml::Value>,
    workspace_dependencies: &BTreeMap<String, DependencyEntry>,
    line: u32,
) -> DependencyEntry {
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

#[instrument(level = "debug", skip(workspace_dependencies))]
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

#[instrument(level = "debug", skip(table))]
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

#[instrument(level = "debug", skip(manifest, section))]
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

#[instrument(level = "debug", skip(section))]
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

#[instrument(level = "debug")]
fn target_headers(target: &str, section: &str) -> Vec<String> {
    vec![
        format!("[target.'{target}'.{section}]"),
        format!("[target.\"{target}\".{section}]"),
    ]
}
