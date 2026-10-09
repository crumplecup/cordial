//! Session pipeline: load, probe, assess, render.

use std::sync::Arc;

use tracing::instrument;

use crate::error::CordialResult;
use crate::etiquette::Etiquette;
use crate::hooks::WorkspaceAssessView;

use crate::objects::{Artifact, Finding};

use crate::store::StoreLayout;

use super::resolve::{
    dedupe_assessors, dedupe_enrichers, dedupe_loaders, dedupe_probes, dedupe_reporters,
    dedupe_workspace_assessors, resolved_etiquettes,
};
use super::{RunFilter, RunOutcome, RuntimeSession, SessionView};

mod assess;
#[cfg(any(
    feature = "quality",
    feature = "homecoming_std",
    feature = "amenable_std",
    feature = "elicitation"
))]
mod includes;
mod load;
mod render;

struct ConcreteRunOutcome {
    findings: Vec<Box<dyn Finding>>,
    artifacts: Vec<Box<dyn Artifact>>,
}

impl RunOutcome for ConcreteRunOutcome {
    #[instrument(level = "trace", skip(self))]
    fn findings(&self) -> Box<dyn Iterator<Item = &dyn Finding> + '_> {
        Box::new(
            self.findings
                .iter()
                .map(|finding| finding.as_ref() as &dyn Finding),
        )
    }

    #[instrument(level = "trace", skip(self))]
    fn artifacts(&self) -> Box<dyn Iterator<Item = &dyn Artifact> + '_> {
        Box::new(
            self.artifacts
                .iter()
                .map(|artifact| artifact.as_ref() as &dyn Artifact),
        )
    }
}

#[instrument(level = "debug")]
fn empty_outcome() -> Box<dyn RunOutcome> {
    Box::new(ConcreteRunOutcome {
        findings: Vec::new(),
        artifacts: Vec::new(),
    })
}

#[instrument(level = "info", skip(session, filter), err(level = "warn"))]
pub(super) fn run_session(
    session: &RuntimeSession,
    filter: &dyn RunFilter,
) -> CordialResult<Box<dyn RunOutcome>> {
    let store = StoreLayout::from_root(
        session.store_root(),
        crate::store::project_slug_from_path(session.project_root()),
    );
    store.ensure_dirs()?;

    let setup = session
        .progress()
        .spinner("Preparing cordial run".to_string());
    let etiquettes = resolved_etiquettes(
        session.registered_plugins(),
        session.registered_etiquettes(),
        filter,
        session,
    );
    let config = crate::load_session_config(session);
    let etiquettes: Vec<Arc<dyn Etiquette>> = etiquettes
        .into_iter()
        .filter(|etiquette| config.etiquette_enabled(etiquette.id()))
        .collect();
    if etiquettes.is_empty() {
        setup.finish("No enabled etiquettes".to_string());
        return Ok(empty_outcome());
    }

    let targets = crate::targets::discover_run_crate_targets(
        session.registered_plugins(),
        session.project_root(),
        session,
        filter,
    )?;
    if targets.is_empty() {
        setup.finish("No matching crates".to_string());
        return Ok(empty_outcome());
    }

    let loaders = dedupe_loaders(&etiquettes);
    let enrichers = dedupe_enrichers(&etiquettes, &loaders);
    let probes = dedupe_probes(&etiquettes);
    let assessors = dedupe_assessors(&etiquettes);
    let workspace_assessors = dedupe_workspace_assessors(&etiquettes);
    let reporters = dedupe_reporters(&etiquettes);
    setup.finish(format!(
        "Running {} etiquette(s) on {} crate(s)",
        etiquettes.len(),
        targets.len()
    ));

    let loaded = load::load_and_probe(
        session, filter, &store, &targets, &loaders, &enrichers, &probes,
    )?;
    #[cfg(feature = "shadow")]
    let (mut workspace, markers_by_crate) = loaded.into_parts();
    #[cfg(not(feature = "shadow"))]
    let (workspace, markers_by_crate) = loaded.into_parts();

    let etiquette_ids: Vec<&str> = etiquettes.iter().map(|etiquette| etiquette.id()).collect();
    let mut all_findings = assess::assess_targets(
        session,
        &store,
        &targets,
        &workspace,
        &markers_by_crate,
        &assessors,
        &etiquette_ids,
    )?;

    #[cfg(feature = "shadow")]
    {
        let preload = session
            .progress()
            .spinner("Preparing shadow workspace inputs".to_string());
        crate::shadow::preload_shadow_pair_crates(
            &mut workspace,
            session,
            filter,
            &loaders,
            &enrichers,
        )?;
        preload.finish("Prepared shadow workspace inputs".to_string());
    }

    if !workspace_assessors.is_empty() {
        let task = session.progress().bar(
            "Assessing workspace rules".to_string(),
            workspace_assessors.len() as u64,
        );
        for assessor in &workspace_assessors {
            task.set_message(format!("Assessing workspace rule {}", assessor.id()));
            all_findings
                .extend(assessor.assess(WorkspaceAssessView::new(&workspace, session, filter))?);
            task.inc(1);
        }
        task.finish(format!(
            "Assessed {} workspace rule(s)",
            workspace_assessors.len()
        ));
    }

    let all_artifacts = render::render_and_write(
        session,
        filter,
        render::RenderPass::new(
            &store,
            &targets,
            &workspace,
            &etiquettes,
            &etiquette_ids,
            &reporters,
            &all_findings,
        ),
    )?;

    Ok(Box::new(ConcreteRunOutcome {
        findings: all_findings,
        artifacts: all_artifacts,
    }))
}
