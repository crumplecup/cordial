//! Render reports and write the artifacts.

use std::sync::Arc;

use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::etiquette::Etiquette;
use crate::hooks::{RenderView, Reporter};
use crate::ir::{CrateView, WorkspaceIr};
use crate::loader::CrateTarget;
use crate::objects::{Artifact, Finding};
use crate::reporter::RollupReporter;
use crate::store::StoreLayout;

#[cfg(any(
    feature = "homecoming_std",
    feature = "amenable_std",
    feature = "elicitation"
))]
use super::includes::run_includes_coverage;
#[cfg(feature = "quality")]
use super::includes::run_includes_quality;
use crate::session::{RunFilter, RuntimeSession, SessionView};

/// Everything one render pass reads, bundled to keep the call readable.
#[derive(derive_new::new)]
pub(super) struct RenderPass<'a> {
    store: &'a StoreLayout,
    targets: &'a [CrateTarget],
    workspace: &'a WorkspaceIr,
    etiquettes: &'a [Arc<dyn Etiquette>],
    etiquette_ids: &'a [&'a str],
    reporters: &'a [&'a dyn Reporter],
    findings: &'a [Box<dyn Finding>],
}

#[instrument(level = "debug", skip(session, filter, pass), err(level = "warn"))]
pub(super) fn render_and_write(
    session: &RuntimeSession,
    filter: &dyn RunFilter,
    pass: RenderPass<'_>,
) -> CordialResult<Vec<Box<dyn Artifact>>> {
    let RenderPass {
        store,
        targets,
        workspace,
        etiquettes,
        etiquette_ids,
        reporters,
        findings: all_findings,
    } = pass;
    let primary_name = targets
        .first()
        .map(|target| target.crate_name().clone())
        .ok_or_else(|| CordialError::invariant("workspace missing crate targets"))?;
    let crate_view = CrateView::new(workspace, primary_name);

    let finding_refs: Vec<&dyn Finding> = all_findings
        .iter()
        .map(|finding| finding.as_ref() as &dyn Finding)
        .collect();

    let progress = session.progress().spinner("Writing reports".to_string());
    let mut all_artifacts: Vec<Box<dyn Artifact>> = Vec::new();
    for reporter in reporters {
        let mut artifacts =
            reporter.render(RenderView::new(&finding_refs, &crate_view, session))?;
        all_artifacts.append(&mut artifacts);
    }

    let rollup = RollupReporter;
    let mut rollup_artifacts =
        rollup.render(RenderView::new(&finding_refs, &crate_view, session))?;
    all_artifacts.append(&mut rollup_artifacts);

    #[cfg(feature = "quality")]
    let includes_quality = run_includes_quality(session.registered_plugins(), filter, etiquettes);
    #[cfg(all(
        not(feature = "quality"),
        any(
            feature = "homecoming_std",
            feature = "amenable_std",
            feature = "elicitation"
        )
    ))]
    let includes_quality = false;

    #[cfg(feature = "quality")]
    if includes_quality {
        let quality_report = crate::reporter::QualityReportReporter;
        let mut quality_artifacts =
            quality_report.render(RenderView::new(&finding_refs, &crate_view, session))?;
        all_artifacts.append(&mut quality_artifacts);
    }

    #[cfg(any(
        feature = "homecoming_std",
        feature = "amenable_std",
        feature = "elicitation"
    ))]
    if run_includes_coverage(session.registered_plugins(), filter, etiquettes) {
        crate::reporter::attach_coverage_summary_artifacts(
            crate::reporter::CoverageSummaryPass::new(
                session.registered_plugins(),
                etiquette_ids,
                &finding_refs,
                workspace,
                includes_quality,
            ),
            filter,
            session,
            &mut all_artifacts,
        )?;
    }
    #[cfg(not(any(
        feature = "homecoming_std",
        feature = "amenable_std",
        feature = "elicitation"
    )))]
    let _ = (filter, etiquettes, etiquette_ids);

    for artifact in &all_artifacts {
        let path = store.findings_dir().join(artifact.name());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&path)?;
        artifact.write_to(&mut file)?;
    }

    progress.finish(format!("Wrote {} report artifact(s)", all_artifacts.len()));
    Ok(all_artifacts)
}
