use std::collections::HashSet;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::ir::CrateKind;
use crate::loader::CrateTarget;
use crate::plugin::{PluginCategory, plugins_in_category, selected_plugins};
use crate::session::{RunAll, RunFilter, SessionView};

/// Discover crate targets for a session run.
///
/// A project is a Cargo crate or workspace, so the root must contain a
/// manifest; `cargo metadata` supplies the targets.
#[instrument(level = "debug", skip(filter), err(level = "warn"))]
pub fn discover_crate_targets(
    project_root: &Path,
    filter: &dyn RunFilter,
) -> CordialResult<Vec<CrateTarget>> {
    let manifest = project_root.join("Cargo.toml");
    if !manifest.is_file() {
        return Err(CordialError::not_found(manifest));
    }
    let mut targets = workspace_targets(project_root)?;

    targets = apply_target_filter(targets, filter);
    targets.sort_by(|a, b| a.crate_name().cmp(b.crate_name()));
    Ok(targets)
}

/// Discover crate targets for a session run, driven by active coverage plugins when present.
///
/// When one or more [`PluginCategory::Coverage`] plugins are selected, crate names come from
/// each coverage plugin's target list. Quality-only runs continue to use workspace
/// members from `cargo metadata`. Combined quality + coverage runs union coverage
/// IR crate names with all filtered workspace members.
#[instrument(
    level = "debug",
    skip(registered_plugins, session, filter),
    err(level = "warn")
)]
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
    let coverage_crate_names: HashSet<String> = {
        // Without `rustdoc`, coverage-plugin crate names come from IR this
        // build can't load -- `session` is real input but genuinely
        // unneeded in this branch.
        let _ = session;
        HashSet::new()
    };

    let mut crate_names = coverage_crate_names;
    if !plugins_in_category(&active_plugins, PluginCategory::Quality).is_empty()
        || !plugins_in_category(&active_plugins, PluginCategory::ErrorHandling).is_empty()
    {
        for target in &filtered_workspace_targets {
            crate_names.insert(target.crate_name().clone());
        }
    }

    if crate_names.is_empty() {
        return Ok(filtered_workspace_targets);
    }

    let mut targets: Vec<CrateTarget> = all_workspace_targets
        .into_iter()
        .filter(|target| crate_names.contains(target.crate_name()))
        .collect();
    targets.sort_by(|a, b| a.crate_name().cmp(b.crate_name()));
    Ok(targets)
}

#[instrument(level = "debug", err(level = "warn"))]
fn workspace_targets(project_root: &Path) -> CordialResult<Vec<CrateTarget>> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(project_root)
        .exec()
        .map_err(|err| CordialError::cargo_metadata(err))?;

    let mut targets = Vec::new();
    for package_id in &metadata.workspace_members {
        let package = metadata
            .packages
            .iter()
            .find(|candidate| &candidate.id == package_id)
            .ok_or_else(|| CordialError::missing_package_metadata(package_id.to_string()))?;
        let crate_root = package.manifest_path.parent().ok_or_else(|| {
            CordialError::manifest_path_has_no_parent(package.manifest_path.to_string())
        })?;
        targets.push(
            CrateTarget::new(package.name.to_string(), PathBuf::from(crate_root.as_str()))
                .with_kinds(package_kinds(package))
                .with_binary_entries(
                    package
                        .targets
                        .iter()
                        .filter(|target| target.kind.contains(&cargo_metadata::TargetKind::Bin))
                        .map(|target| target.src_path.clone().into_std_path_buf())
                        .collect(),
                )
                .with_manifest_snapshot(Some(crate::ir::FileSnapshot::new(
                    package.manifest_path.clone().into_std_path_buf(),
                    Sha256::digest(std::fs::read(&package.manifest_path)?).into(),
                ))),
        );
    }

    if targets.is_empty() {
        return Err(CordialError::no_workspace_members_found(
            project_root.to_path_buf(),
        ));
    }
    Ok(targets)
}

/// Crate kinds from Cargo's own target list, sorted and de-duplicated.
///
/// Cargo already resolves `[lib] proc-macro = true` (and its spelling
/// variants) into a `proc-macro` target kind, so no manifest parsing is
/// needed here.
#[instrument(level = "debug", skip(package))]
fn package_kinds(package: &cargo_metadata::Package) -> Vec<CrateKind> {
    use cargo_metadata::TargetKind;

    let mut kinds: Vec<CrateKind> = package
        .targets
        .iter()
        .flat_map(|target| target.kind.iter())
        .filter_map(|kind| match kind {
            TargetKind::Lib
            | TargetKind::RLib
            | TargetKind::DyLib
            | TargetKind::CDyLib
            | TargetKind::StaticLib => Some(CrateKind::Lib),
            TargetKind::Bin => Some(CrateKind::Bin),
            TargetKind::ProcMacro => Some(CrateKind::ProcMacro),
            _ => None,
        })
        .collect();
    kinds.sort();
    kinds.dedup();
    kinds
}

#[instrument(level = "debug", skip(targets, filter))]
fn apply_target_filter(targets: Vec<CrateTarget>, filter: &dyn RunFilter) -> Vec<CrateTarget> {
    if let Some(name) = filter.crate_name() {
        return targets
            .into_iter()
            .filter(|target| target.crate_name() == name)
            .collect();
    }
    if let Some(names) = filter.crates() {
        return targets
            .into_iter()
            .filter(|target| names.iter().any(|name| *name == target.crate_name()))
            .collect();
    }
    targets
}
