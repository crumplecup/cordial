use std::collections::HashSet;
use std::path::{Path, PathBuf};

use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::loader::CrateTarget;
use crate::plugin::{PluginCategory, plugins_in_category, selected_plugins};
use crate::session::{RunAll, RunFilter, SessionView};
use crate::store::project_slug_from_path;

/// Discover crate targets for a session run.
///
/// Uses `cargo metadata` when the project root contains a manifest; otherwise
/// falls back to a single synthetic target named after the directory.
#[instrument(skip(filter), fields(project_root = %project_root.display()))]
pub fn discover_crate_targets(
    project_root: &Path,
    filter: &dyn RunFilter,
) -> CordialResult<Vec<CrateTarget>> {
    let mut targets = if project_root.join("Cargo.toml").is_file() {
        workspace_targets(project_root)?
    } else {
        vec![CrateTarget::new(
            project_slug_from_path(project_root),
            project_root,
        )]
    };

    targets = apply_target_filter(targets, filter);
    targets.sort_by(|a, b| a.crate_name.cmp(&b.crate_name));
    Ok(targets)
}

/// Discover crate targets for a session run, driven by active coverage plugins when present.
///
/// When one or more [`PluginCategory::Coverage`] plugins are selected, crate names come from
/// each plugin's [`Coverage::targets`](crate::plugin::Coverage::targets) union. Quality-only
/// runs continue to use workspace members from `cargo metadata`. Combined quality + coverage
/// runs union coverage IR crate names with all filtered workspace members.
#[instrument(skip(session, filter, registered_plugins), fields(project_root = %project_root.display()))]
pub fn discover_run_crate_targets(
    registered_plugins: &[&'static dyn crate::plugin::Plugin],
    project_root: &Path,
    session: &dyn SessionView,
    filter: &dyn RunFilter,
) -> CordialResult<Vec<CrateTarget>> {
    let all_workspace_targets = discover_crate_targets(project_root, &RunAll)?;
    let filtered_workspace_targets = discover_crate_targets(project_root, filter)?;
    let active_plugins = selected_plugins(registered_plugins, filter.plugins());
    let coverage_plugins = plugins_in_category(&active_plugins, PluginCategory::Coverage);

    if coverage_plugins.is_empty() {
        return Ok(filtered_workspace_targets);
    }

    #[cfg(feature = "rustdoc")]
    let coverage_crate_names: HashSet<String> =
        crate::plugins::ir_crate_names_for_coverage_plugins(registered_plugins, session, filter)?
            .into_iter()
            .collect();
    #[cfg(not(feature = "rustdoc"))]
    let coverage_crate_names: HashSet<String> = HashSet::new();

    let mut crate_names = coverage_crate_names;
    if !plugins_in_category(&active_plugins, PluginCategory::Quality).is_empty()
        || !plugins_in_category(&active_plugins, PluginCategory::ErrorHandling).is_empty()
    {
        for target in &filtered_workspace_targets {
            crate_names.insert(target.crate_name.clone());
        }
    }

    if crate_names.is_empty() {
        return Ok(filtered_workspace_targets);
    }

    let mut targets: Vec<CrateTarget> = all_workspace_targets
        .into_iter()
        .filter(|target| crate_names.contains(&target.crate_name))
        .collect();
    targets.sort_by(|a, b| a.crate_name.cmp(&b.crate_name));
    Ok(targets)
}

fn workspace_targets(project_root: &Path) -> CordialResult<Vec<CrateTarget>> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(project_root)
        .exec()
        .map_err(CordialError::cargo_metadata)?;

    let mut targets = Vec::new();
    for package_id in &metadata.workspace_members {
        let package = metadata
            .packages
            .iter()
            .find(|candidate| &candidate.id == package_id)
            .ok_or_else(|| {
                CordialError::invariant(format!("missing package metadata for {package_id}"))
            })?;
        let crate_root = package.manifest_path.parent().ok_or_else(|| {
            CordialError::invariant(format!(
                "manifest path has no parent: {}",
                package.manifest_path
            ))
        })?;
        targets.push(CrateTarget::new(
            package.name.to_string(),
            PathBuf::from(crate_root.as_str()),
        ));
    }

    if targets.is_empty() {
        return Err(CordialError::invariant(format!(
            "no workspace members found at {}",
            project_root.display()
        )));
    }
    Ok(targets)
}

fn apply_target_filter(targets: Vec<CrateTarget>, filter: &dyn RunFilter) -> Vec<CrateTarget> {
    if let Some(name) = filter.crate_name() {
        return targets
            .into_iter()
            .filter(|target| target.crate_name == name)
            .collect();
    }
    if let Some(names) = filter.crates() {
        return targets
            .into_iter()
            .filter(|target| names.iter().any(|name| *name == target.crate_name))
            .collect();
    }
    targets
}

#[cfg(test)]
mod tests {
    use std::fs;

    use miette::{IntoDiagnostic, WrapErr};

    use crate::NamedRunFilter;

    use super::*;

    #[test]
    fn non_cargo_project_uses_directory_slug() -> miette::Result<()> {
        let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
        let targets = discover_crate_targets(fixture.path(), &NamedRunFilter::all_etiquettes())
            .into_diagnostic()
            .wrap_err("targets")?;
        assert_eq!(targets.len(), 1);
        assert_eq!(
            targets[0].crate_name,
            project_slug_from_path(fixture.path())
        );
        Ok(())
    }

    #[test]
    fn workspace_members_discovered_from_manifest() -> miette::Result<()> {
        let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
        write_workspace(
            fixture.path(),
            r#"
[workspace]
members = ["crates/alpha", "crates/beta"]
resolver = "2"
"#,
        )?;
        write_member_crate(fixture.path(), "crates/alpha", "alpha")?;
        write_member_crate(fixture.path(), "crates/beta", "beta")?;

        let targets = discover_crate_targets(fixture.path(), &NamedRunFilter::all_etiquettes())
            .into_diagnostic()
            .wrap_err("targets")?;
        assert_eq!(targets.len(), 2);
        assert!(targets.iter().any(|target| target.crate_name == "alpha"));
        assert!(targets.iter().any(|target| target.crate_name == "beta"));
        Ok(())
    }

    #[test]
    fn crate_name_filter_selects_one_member() -> miette::Result<()> {
        let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
        write_workspace(
            fixture.path(),
            r#"
[workspace]
members = ["crates/alpha", "crates/beta"]
resolver = "2"
"#,
        )?;
        write_member_crate(fixture.path(), "crates/alpha", "alpha")?;
        write_member_crate(fixture.path(), "crates/beta", "beta")?;

        let filter = NamedRunFilter::all_etiquettes().with_crate("beta");
        let targets = discover_crate_targets(fixture.path(), &filter)
            .into_diagnostic()
            .wrap_err("targets")?;
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].crate_name, "beta");
        Ok(())
    }

    #[test]
    #[cfg(feature = "elicitation")]
    fn coverage_plugin_shadow_pair_includes_both_crates_when_filtered_to_upstream()
    -> miette::Result<()> {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/parity/workspaces/minimal-workspace");
        let session = crate::SessionBuilder::new(&fixture).build();
        let filter = crate::NamedRunFilter::all_plugins().with_crate("url".to_string());
        let plugins = vec![&crate::plugins::ELICITATION_COVERAGE as &dyn crate::plugin::Plugin];

        let targets = discover_run_crate_targets(&plugins, &fixture, &session, &filter)
            .into_diagnostic()
            .wrap_err("targets")?;
        let names: HashSet<String> = targets.into_iter().map(|t| t.crate_name).collect();
        assert!(names.contains("url"));
        assert!(
            names.contains("elicit_url"),
            "shadow pair should schedule shadow crate IR when filtering upstream"
        );
        Ok(())
    }

    fn write_workspace(root: &Path, body: &str) -> miette::Result<()> {
        fs::write(root.join("Cargo.toml"), body)
            .into_diagnostic()
            .wrap_err("workspace manifest")?;
        Ok(())
    }

    fn write_member_crate(root: &Path, rel: &str, name: &str) -> miette::Result<()> {
        let crate_root = root.join(rel);
        fs::create_dir_all(crate_root.join("src"))
            .into_diagnostic()
            .wrap_err("src dir")?;
        fs::write(
            crate_root.join("Cargo.toml"),
            format!(
                r#"
[package]
name = "{name}"
version = "0.1.0"
edition = "2024"
"#
            ),
        )
        .into_diagnostic()
        .wrap_err("member manifest")?;
        fs::write(crate_root.join("src/lib.rs"), "pub fn ok() {}")
            .into_diagnostic()
            .wrap_err("lib rs")?;
        Ok(())
    }
}
