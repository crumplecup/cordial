mod entry;
mod freshness;
mod lockfile;

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use crate::error::{CordialError, CordialResult};
use crate::loader::CrateTarget;

use super::types::{DependencySurveyRecord, DependencySurveyRecordInput};

use self::entry::{dependency_entries, workspace_dependencies};
use self::freshness::{
    add_freshness_observations, freshness_index, record_can_have_freshness_observations,
};
use self::lockfile::{locked_versions_for_entry, lockfile_index};

use tracing::instrument;

/// Survey selected crates from one workspace snapshot and join Cargo freshness once.
#[instrument(level = "debug", skip(targets))]
pub(crate) fn survey_workspace_dependency_freshness(
    project_root: &Path,
    targets: &[CrateTarget],
    store_root: Option<&Path>,
) -> CordialResult<HashMap<String, Vec<DependencySurveyRecord>>> {
    let root_manifest_path = project_root.join("Cargo.toml");
    let root_manifest = if root_manifest_path.is_file() {
        let source = fs::read_to_string(&root_manifest_path)?;
        let table = parse_table(&source, &root_manifest_path)?;
        Some((source, table))
    } else {
        None
    };
    let workspace_dependencies = root_manifest
        .as_ref()
        .map(|(source, table)| workspace_dependencies(source, table))
        .unwrap_or_default();
    let lockfile_index = lockfile_index(project_root)?;
    let mut by_crate = HashMap::new();
    for target in targets {
        let manifest_path = target.crate_root().join("Cargo.toml");
        let (manifest, manifest_table) = if manifest_path == root_manifest_path {
            root_manifest.as_ref().ok_or_else(|| {
                CordialError::workspace_source_facts_missing("workspace Cargo.toml")
            })?
        } else {
            // Member manifests are independent inputs; each is read and parsed once.
            let manifest = fs::read_to_string(&manifest_path)?;
            let table = parse_table(&manifest, &manifest_path)?;
            let records = survey_manifest(
                target.crate_name(),
                &manifest_path,
                &manifest,
                &table,
                &workspace_dependencies,
                &lockfile_index,
            )?;
            by_crate.insert(target.crate_name().clone(), records);
            continue;
        };
        let records = survey_manifest(
            target.crate_name(),
            &manifest_path,
            manifest,
            manifest_table,
            &workspace_dependencies,
            &lockfile_index,
        )?;
        by_crate.insert(target.crate_name().clone(), records);
    }
    if by_crate
        .values()
        .flatten()
        .any(record_can_have_freshness_observations)
    {
        let freshness_index = freshness_index(project_root, store_root)?;
        for record in by_crate.values_mut().flatten() {
            add_freshness_observations(record, &freshness_index);
        }
    }

    Ok(by_crate)
}

#[instrument(
    level = "debug",
    skip(manifest, manifest_table, workspace_dependencies, lockfile_index)
)]
fn survey_manifest(
    crate_name: &str,
    manifest_path: &Path,
    manifest: &str,
    manifest_table: &toml::Table,
    workspace_dependencies: &BTreeMap<String, entry::DependencyEntry>,
    lockfile_index: &lockfile::LockfileIndex,
) -> CordialResult<Vec<DependencySurveyRecord>> {
    let mut records = Vec::new();
    for entry in dependency_entries(manifest, manifest_table, workspace_dependencies) {
        let input = DependencySurveyRecordInput::builder()
            .crate_name(crate_name.to_string())
            .dependency_name(entry.dependency_name().clone())
            .package_name(entry.package_name().clone())
            .manifest_path(manifest_path.to_path_buf())
            .line(entry.line())
            .section(entry.section().clone())
            .version_spec(entry.version_spec().clone())
            .source_kind(entry.source_kind().clone())
            .locked_versions(locked_versions_for_entry(&entry, lockfile_index))
            .indicators(entry.indicators().clone())
            .build()?;
        records.push(DependencySurveyRecord::from_input(input));
    }
    Ok(records)
}

#[instrument(level = "debug", skip(path))]
fn parse_table(content: &str, path: &Path) -> CordialResult<toml::Table> {
    toml::from_str(content)
        .map_err(|error| CordialError::toml_parse(path.display().to_string(), error))
}
