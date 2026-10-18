//! Session pipeline: load, probe, assess, render.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use tracing::instrument;

use crate::error::CordialResult;
use crate::etiquette::Etiquette;
use crate::hooks::WorkspaceAssessView;

use crate::objects::{Artifact, Finding};

use crate::store::StoreLayout;
use crate::{GateLedger, HookTiming, HookTimings, TierSelection};

use super::resolve::{
    dedupe_assessors, dedupe_enrichers, dedupe_loaders, dedupe_probes, dedupe_reporters,
    dedupe_workspace_assessors, resolved_etiquettes,
};
use super::{RunFilter, RunOutcome, RuntimeSession, SessionView, assess, load, render};

struct ConcreteRunOutcome {
    findings: Vec<Box<dyn Finding>>,
    artifacts: Vec<Box<dyn Artifact>>,
    timings: Vec<HookTiming>,
    skipped_tiers: Vec<String>,
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

    #[instrument(level = "trace", skip(self))]
    fn hook_timings(&self) -> &[HookTiming] {
        &self.timings
    }

    #[instrument(level = "trace", skip(self))]
    fn skipped_tiers(&self) -> &[String] {
        &self.skipped_tiers
    }
}

#[instrument(level = "debug")]
fn empty_outcome() -> Box<dyn RunOutcome> {
    Box::new(ConcreteRunOutcome {
        findings: Vec::new(),
        artifacts: Vec::new(),
        timings: Vec::new(),
        skipped_tiers: Vec::new(),
    })
}

#[instrument(level = "info", skip(session, filter), err(level = "warn"))]
#[hotpath::measure]
pub(super) fn run_session(
    session: &RuntimeSession,
    filter: &dyn RunFilter,
) -> CordialResult<Box<dyn RunOutcome>> {
    let run_started = Instant::now();
    let store = StoreLayout::from_root(
        session.store_root(),
        crate::store::project_slug_from_path(session.project_root()),
    );
    store.ensure_dirs()?;

    let setup = session
        .progress()
        .spinner("Preparing cordial run".to_string());
    let available = resolved_etiquettes(
        session.registered_plugins(),
        session.registered_etiquettes(),
        filter,
        session,
    );
    let config = crate::load_session_config(session);
    let selection = TierSelection::resolve(&available, &config, filter.tiers())?;
    let skipped_tiers: Vec<String> = available
        .iter()
        .filter(|etiquette| config.etiquette_enabled(etiquette.id()))
        .filter(|etiquette| !selection.includes(etiquette.as_ref(), &config))
        .map(|etiquette| {
            crate::assigned_tier(etiquette.as_ref(), &config)
                .as_str()
                .to_string()
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let explicit = filter.etiquettes().is_some();
    let etiquettes: Vec<Arc<dyn Etiquette>> = available
        .into_iter()
        .filter(|etiquette| {
            config.etiquette_enabled(etiquette.id())
                && (explicit || selection.includes(etiquette.as_ref(), &config))
        })
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

    let mut timings = HookTimings::default();
    timings.record("phase", "setup", run_started.elapsed());
    let started = Instant::now();
    let loaded = load::load_and_probe(
        session,
        filter,
        &store,
        &targets,
        &loaders,
        &enrichers,
        &probes,
        &mut timings,
    );
    timings.record("phase", "load_and_probe", started.elapsed());
    let loaded = loaded?;
    #[cfg(feature = "shadow")]
    let (mut workspace, markers_by_crate) = loaded.into_parts();
    #[cfg(not(feature = "shadow"))]
    let (workspace, markers_by_crate) = loaded.into_parts();

    let etiquette_ids: Vec<&str> = etiquettes.iter().map(|etiquette| etiquette.id()).collect();
    let started = Instant::now();
    let all_findings = assess::assess_targets(
        session,
        &store,
        &targets,
        &workspace,
        &markers_by_crate,
        &assessors,
        &etiquette_ids,
        &mut timings,
    );
    timings.record("phase", "assess", started.elapsed());
    let mut all_findings = all_findings?;

    #[cfg(feature = "shadow")]
    {
        let preload = session
            .progress()
            .spinner("Preparing shadow workspace inputs".to_string());
        timings.measure("phase", "shadow_preload", || {
            crate::shadow::preload_shadow_pair_crates(
                &mut workspace,
                session,
                filter,
                &loaders,
                &enrichers,
            )
        })?;
        preload.finish("Prepared shadow workspace inputs".to_string());
    }

    if !workspace_assessors.is_empty() {
        let started = Instant::now();
        let task = session.progress().bar(
            "Assessing workspace rules".to_string(),
            workspace_assessors.len() as u64,
        );
        for assessor in &workspace_assessors {
            task.set_message(format!("Assessing workspace rule {}", assessor.id()));
            all_findings.extend(timings.measure("workspace_assess", assessor.id(), || {
                assessor.assess(WorkspaceAssessView::new(&workspace, session, filter))
            })?);
            task.inc(1);
        }
        task.finish(format!(
            "Assessed {} workspace rule(s)",
            workspace_assessors.len()
        ));
        timings.record("phase", "workspace_assess", started.elapsed());
    }

    let started = Instant::now();
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
        &mut timings,
    );
    timings.record("phase", "render_and_write", started.elapsed());
    let all_artifacts = all_artifacts?;

    if !session.measurement_mode() {
        let started = Instant::now();
        let task = session
            .progress()
            .spinner("Updating gate ledger".to_string());
        let mut ledger = GateLedger::read(&store)?;
        ledger.record_run(&etiquettes, &all_findings, &timings, &config)?;
        #[cfg(feature = "quality")]
        {
            let finding_refs: Vec<&dyn Finding> = all_findings
                .iter()
                .map(|finding| finding.as_ref())
                .collect();
            let report = crate::build_quality_report(&finding_refs)?;
            ledger.record_quality_areas(&report, &etiquette_ids);
        }
        timings.record("phase", "ledger", started.elapsed());
        ledger.record_last_run_timings(timings.clone().into_vec(), run_started.elapsed());
        ledger.write(&store)?;
        task.finish("Updated gate ledger".to_string());
    }

    Ok(Box::new(ConcreteRunOutcome {
        findings: all_findings,
        artifacts: all_artifacts,
        timings: timings.into_vec(),
        skipped_tiers,
    }))
}
