//! Render reports and write the artifacts.

#[cfg(feature = "quality")]
use std::collections::BTreeSet;
use std::sync::Arc;

use tracing::instrument;

use crate::HookTimings;
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

#[instrument(
    level = "debug",
    skip(session, filter, pass, timings),
    err(level = "warn")
)]
#[hotpath::measure]
pub(super) fn render_and_write(
    session: &RuntimeSession,
    filter: &dyn RunFilter,
    pass: RenderPass<'_>,
    timings: &mut HookTimings,
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

    let progress = session.progress().bar(
        "Rendering reports".to_string(),
        (reporters.len() + 1) as u64,
    );
    let mut all_artifacts: Vec<Box<dyn Artifact>> = Vec::new();
    for reporter in reporters {
        progress.set_message(format!("Rendering {}", reporter.id()));
        let mut artifacts = timings.measure("report", reporter.id(), || {
            reporter.render(RenderView::new(&finding_refs, &crate_view, session))
        })?;
        all_artifacts.append(&mut artifacts);
        progress.inc(1);
    }

    let rollup = RollupReporter;
    progress.set_message(format!("Rendering {}", rollup.id()));
    let mut rollup_artifacts = timings.measure("report", rollup.id(), || {
        rollup.render(RenderView::new(&finding_refs, &crate_view, session))
    })?;
    all_artifacts.append(&mut rollup_artifacts);
    progress.inc(1);
    progress.finish("Rendered primary reports".to_string());

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
        let task = session
            .progress()
            .spinner("Rendering quality summary".to_string());
        let quality_report = crate::reporter::QualityReportReporter;
        let fresh_artifacts: BTreeSet<String> = all_artifacts
            .iter()
            .map(|artifact| artifact.name().to_string())
            .collect();
        let config = crate::load_session_config(session);
        let ledger = crate::GateLedger::read(store)?;
        let mut quality_artifacts = timings.measure("report", quality_report.id(), || {
            quality_report.render_run(
                &finding_refs,
                etiquette_ids,
                &config,
                &ledger,
                &fresh_artifacts,
                &store.findings_dir(),
            )
        })?;
        all_artifacts.append(&mut quality_artifacts);
        task.finish("Rendered quality summary".to_string());
    }

    #[cfg(any(
        feature = "homecoming_std",
        feature = "amenable_std",
        feature = "elicitation"
    ))]
    if run_includes_coverage(session.registered_plugins(), filter, etiquettes) {
        let task = session
            .progress()
            .spinner("Rendering coverage summary".to_string());
        timings.measure("pipeline", "coverage_summary", || {
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
            )
        })?;
        task.finish("Rendered coverage summary".to_string());
    }
    #[cfg(not(any(
        feature = "homecoming_std",
        feature = "amenable_std",
        feature = "elicitation"
    )))]
    let _ = (filter, etiquettes, etiquette_ids);

    if !session.measurement_mode() {
        let writes = session.progress().bar(
            "Writing report artifacts".to_string(),
            all_artifacts.len() as u64,
        );
        for artifact in &all_artifacts {
            writes.set_message(format!("Writing {}", artifact.name()));
            timings.measure("pipeline", "artifact_write", || {
                let path = store.findings_dir().join(artifact.name());
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut file = std::fs::File::create(&path)?;
                artifact.write_to(&mut file)
            })?;
            writes.inc(1);
        }
        writes.finish(format!("Wrote {} report artifact(s)", all_artifacts.len()));
    }

    Ok(all_artifacts)
}
