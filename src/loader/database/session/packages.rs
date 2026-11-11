//! Workspace package selection comes from the same Cargo load as the crate graph.

use crate::SemanticPackageContext;
use ra_ap_project_model::{ProjectWorkspace, ProjectWorkspaceKind, TargetKind};
use std::path::Path;

/// Workspace members, which define the context identity.
#[tracing::instrument(level = "debug", skip(workspace))]
pub(super) fn collect(workspace: &ProjectWorkspace) -> Vec<SemanticPackageContext> {
    select(workspace, true)
}

/// Dependency packages of the same Cargo graph. They are not part of the
/// context identity, but their source is in the database and may be projected.
#[tracing::instrument(level = "debug", skip(workspace))]
pub(super) fn collect_external(workspace: &ProjectWorkspace) -> Vec<SemanticPackageContext> {
    select(workspace, false)
}

#[tracing::instrument(level = "debug", skip(workspace))]
fn select(workspace: &ProjectWorkspace, members: bool) -> Vec<SemanticPackageContext> {
    let ProjectWorkspaceKind::Cargo { cargo, .. } = &workspace.kind else {
        return Vec::new();
    };
    let mut members: Vec<_> = cargo
        .packages()
        .filter(|package| cargo[*package].is_member == members)
        .map(|package| {
            let package = &cargo[package];
            let mut entries: Vec<_> = package
                .targets
                .iter()
                .filter(|target| {
                    matches!(
                        cargo[**target].kind,
                        TargetKind::Lib { .. } | TargetKind::Bin
                    )
                })
                .map(|target| AsRef::<Path>::as_ref(&cargo[*target].root).to_path_buf())
                .collect();
            entries.sort();
            entries.dedup();
            SemanticPackageContext::new(
                package.name.clone(),
                AsRef::<Path>::as_ref(package.manifest.parent()).to_path_buf(),
                package
                    .dependencies
                    .iter()
                    .map(|dep| cargo[dep.pkg].name.replace('-', "_"))
                    .collect(),
                entries,
            )
        })
        .collect();
    members.sort_by(|left, right| left.name().cmp(right.name()));
    members
}
