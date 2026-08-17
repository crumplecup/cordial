//! Framework std coverage reporter — homecoming `Code` profile.

use crate::error::{CordialError, CordialResult};
use crate::framework_std::{
    FrameworkStdOptions, HOMECOMING_PATCH_SET, load_framework_skip_map,
    render_framework_checklist_md, render_framework_coverage_csv, render_framework_gaps_csv,
};
use crate::hooks::Reporter;
use crate::ir::IrView;
use crate::objects::{Artifact, Finding, TextArtifact};
use crate::session::SessionView;
use crate::store::StoreLayout;

use super::types::{framework_gaps_from_findings, framework_report_from_findings};

#[derive(Debug, Default, Clone, Copy)]
pub struct HomecomingStdReporter;

impl HomecomingStdReporter {
    pub const ID: &'static str = "homecoming-std-reporter";
}

impl Reporter for HomecomingStdReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(
        &self,
        findings: &[&dyn Finding],
        _ir: &dyn IrView,
        session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let options = FrameworkStdOptions::default();
        let report =
            framework_report_from_findings(findings, options.include_nightly).ok_or_else(|| {
                CordialError::invariant("homecoming std reporter requires assessor findings")
            })?;
        let gaps = framework_gaps_from_findings(findings);
        let store = StoreLayout::from_root(
            session.store_root(),
            crate::store::project_slug_from_path(session.project_root()),
        );
        let skip_map = load_framework_skip_map(&store, HOMECOMING_PATCH_SET);

        Ok(vec![
            artifact(
                "std.csv",
                "text/csv",
                render_framework_coverage_csv(&report)?,
            ),
            artifact(
                "std.checklist.md",
                "text/markdown",
                render_framework_checklist_md(&report, &skip_map)?,
            ),
            artifact(
                "gaps-impl.csv",
                "text/csv",
                render_framework_gaps_csv(&gaps)?,
            ),
        ])
    }
}

fn artifact(name: &str, media_type: &str, body: String) -> Box<dyn Artifact> {
    Box::new(TextArtifact {
        name: name.to_string(),
        media_type: media_type.to_string(),
        body,
    })
}
