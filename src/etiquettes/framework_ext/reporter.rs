//! Reporter for amenable ext (third-party crate) registry coverage — one
//! generic implementation shared by every `amenable-ext-{target}`
//! etiquette instance, parameterized by the upstream crate name.

use crate::error::{CordialError, CordialResult};
use crate::framework_std::{
    load_verifier_skip_map, render_amenable_ext_checklist_md, render_amenable_std_coverage_csv,
    render_amenable_std_gaps_csv,
};
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, TextArtifact};
use crate::store::StoreLayout;

use super::row::{ext_gaps_from_findings, ext_report_from_findings};
use tracing::instrument;

/// Reporter for one amenable-ext target crate's registry coverage.
#[derive(Debug, Clone)]
pub struct ExtReporter {
    id: String,
    category: String,
    patch_set: String,
}

impl ExtReporter {
    /// Build the reporter for `target` (e.g. `"jiff"`).
    #[instrument(level = "debug")]
    pub fn new(target: &str) -> Self {
        Self {
            id: format!("amenable-ext-{target}-reporter"),
            category: super::row::ext_etiquette_id(target),
            patch_set: format!("amenable_ext_{target}"),
        }
    }
}

impl Reporter for ExtReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        &self.id
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings();
        let session = view.session();

        let report =
            ext_report_from_findings(findings, &self.category, false)?.ok_or_else(|| {
                CordialError::invariant("amenable ext reporter requires assessor findings")
            })?;
        let gaps = ext_gaps_from_findings(findings, &self.category);
        let store = StoreLayout::from_root(
            session.store_root(),
            crate::store::project_slug_from_path(session.project_root()),
        );
        let skip_map = load_verifier_skip_map(&store, &self.patch_set);

        Ok(vec![
            artifact(
                &format!("{}.csv", self.category),
                "text/csv",
                render_amenable_std_coverage_csv(&report)?,
            ),
            artifact(
                &format!("{}.checklist.md", self.category),
                "text/markdown",
                render_amenable_ext_checklist_md(&report, &skip_map, &self.patch_set)?,
            ),
            artifact(
                &format!("gaps-{}.csv", self.category),
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
