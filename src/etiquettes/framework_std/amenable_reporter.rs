//! Amenable std registry coverage reporter.

use crate::error::{CordialError, CordialResult};
use crate::framework_std::{
    AMENABLE_PATCH_SET, AmenableStdOptions, load_verifier_skip_map,
    render_amenable_std_checklist_md, render_amenable_std_coverage_csv,
    render_amenable_std_gaps_csv,
};
use crate::hooks::Reporter;
use crate::ir::IrView;
use crate::objects::{Artifact, Finding, TextArtifact};
use crate::session::SessionView;
use crate::store::StoreLayout;

use super::types::{amenable_gaps_from_findings, amenable_report_from_findings};

#[derive(Debug, Default, Clone, Copy)]
pub struct AmenableStdReporter;

impl AmenableStdReporter {
    pub const ID: &'static str = "amenable-std-reporter";
}

impl Reporter for AmenableStdReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(
        &self,
        findings: &[&dyn Finding],
        _ir: &dyn IrView,
        session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let options = AmenableStdOptions::default();
        let report =
            amenable_report_from_findings(findings, options.include_nightly).ok_or_else(|| {
                CordialError::invariant("amenable std reporter requires assessor findings")
            })?;
        let gaps = amenable_gaps_from_findings(findings);
        let store = StoreLayout::from_root(
            session.store_root(),
            crate::store::project_slug_from_path(session.project_root()),
        );
        let skip_map = load_verifier_skip_map(&store, AMENABLE_PATCH_SET);

        Ok(vec![
            artifact(
                "std.csv",
                "text/csv",
                render_amenable_std_coverage_csv(&report)?,
            ),
            artifact(
                "std.checklist.md",
                "text/markdown",
                render_amenable_std_checklist_md(&report, &skip_map)?,
            ),
            artifact(
                "gaps-impl.csv",
                "text/csv",
                render_amenable_std_gaps_csv(&gaps)?,
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
