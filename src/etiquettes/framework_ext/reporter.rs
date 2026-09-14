//! Reporter for amenable ext (jiff) registry coverage — the
//! `amenable_ext` counterpart of
//! `etiquettes::framework_std::amenable_reporter::AmenableStdReporter`.

use crate::error::{CordialError, CordialResult};
use crate::framework_std::{
    AMENABLE_EXT_JIFF_PATCH_SET, load_verifier_skip_map, render_amenable_ext_checklist_md,
    render_amenable_std_coverage_csv, render_amenable_std_gaps_csv,
};
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, TextArtifact};
use crate::store::StoreLayout;

use super::jiff::{amenable_ext_jiff_gaps_from_findings, amenable_ext_jiff_report_from_findings};

use tracing::instrument;
/// Reporter for amenable-ext-jiff coverage.
#[derive(Debug, Default, Clone, Copy)]
pub struct AmenableExtJiffReporter;

impl AmenableExtJiffReporter {
    /// Stable identifier for `AmenableExtJiffReporter`.
    pub const ID: &'static str = "amenable-ext-jiff-reporter";
}

impl Reporter for AmenableExtJiffReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings();
        let session = view.session();

        let report = amenable_ext_jiff_report_from_findings(findings, false).ok_or_else(|| {
            CordialError::invariant("amenable ext jiff reporter requires assessor findings")
        })?;
        let gaps = amenable_ext_jiff_gaps_from_findings(findings);
        let store = StoreLayout::from_root(
            session.store_root(),
            crate::store::project_slug_from_path(session.project_root()),
        );
        let skip_map = load_verifier_skip_map(&store, AMENABLE_EXT_JIFF_PATCH_SET);

        Ok(vec![
            artifact(
                "amenable-ext-jiff.csv",
                "text/csv",
                render_amenable_std_coverage_csv(&report)?,
            ),
            artifact(
                "amenable-ext-jiff.checklist.md",
                "text/markdown",
                render_amenable_ext_checklist_md(&report, &skip_map, AMENABLE_EXT_JIFF_PATCH_SET)?,
            ),
            artifact(
                "gaps-amenable-ext-jiff.csv",
                "text/csv",
                render_amenable_std_gaps_csv(&gaps)?,
            ),
        ])
    }
}

#[instrument(level = "debug")]
fn artifact(name: &str, media_type: &str, body: String) -> Box<dyn Artifact> {
    Box::new(TextArtifact::new(
        name.to_string(),
        media_type.to_string(),
        body,
    ))
}
