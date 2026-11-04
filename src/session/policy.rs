//! Workspace findings use their emitted crate identity to resolve file policy.

use std::path::Path;

use crate::ir::{CrateView, WorkspaceIr};
use crate::loader::CrateTarget;
use crate::objects::{Finding, MapFindingSink};
use crate::{CordialConfig, LintLocation};

#[tracing::instrument(level = "debug", skip(config, targets, workspace, findings))]
pub(super) fn retain_workspace_findings(
    config: &CordialConfig,
    project_root: &Path,
    targets: &[CrateTarget],
    workspace: &WorkspaceIr,
    findings: &mut Vec<Box<dyn Finding>>,
) {
    findings.retain(|finding| {
        let mut sink = MapFindingSink::default();
        finding.emit(&mut sink);
        let target = sink
            .fields()
            .iter()
            .find(|(key, _)| key == "crate" || key == "target_crate")
            .and_then(|(_, name)| targets.iter().find(|target| target.crate_name() == name));
        let view = target.map(|target| CrateView::new(workspace, target.crate_name().clone()));
        let location = LintLocation::new(
            project_root,
            target.map(|target| target.crate_root().as_path()),
            target.map_or("", |target| target.crate_name()),
            None,
        );
        config.finding_enabled(
            finding.as_ref(),
            location,
            view.as_ref().map(|view| view as &dyn crate::ir::IrView),
        )
    });
}
