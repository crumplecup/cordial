mod entry;
mod freshness;
mod lockfile;

use std::fs;
use std::path::Path;

use crate::error::{CordialError, CordialResult};

use super::types::{DependencySurveyRecord, DependencySurveyRecordInput};

use self::entry::{dependency_entries, workspace_dependencies};
use self::freshness::{
    add_freshness_observations, freshness_index, record_can_have_freshness_observations,
};
use self::lockfile::{locked_versions_for_entry, lockfile_index};

use tracing::instrument;

/// Survey one crate and join Cargo registry freshness facts.
#[instrument(level = "debug")]
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
        let input = DependencySurveyRecordInput::builder()
            .crate_name(crate_name.to_string())
            .dependency_name(entry.dependency_name().clone())
            .package_name(entry.package_name().clone())
            .manifest_path(manifest_path.clone())
            .line(entry.line())
            .section(entry.section().clone())
            .version_spec(entry.version_spec().clone())
            .source_kind(entry.source_kind().clone())
            .locked_versions(locked_versions)
            .indicators(entry.indicators().clone())
            .build()?;
        let record = DependencySurveyRecord::from_input(input);
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

#[instrument(level = "debug", skip(path))]
fn parse_table(content: &str, path: &Path) -> CordialResult<toml::Table> {
    toml::from_str(content).map_err(|error| {
        CordialError::invariant(format!("failed to parse {}: {error}", path.display()))
    })
}
