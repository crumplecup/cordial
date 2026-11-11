//! Fresh apply evidence from the same loading, enrichment, and assessment hooks.

use std::collections::BTreeMap;

use super::{RunFilter, RuntimeSession, SessionView, assess, load, resolve};
use crate::{CordialResult, Finding, HookTimings, StoreLayout, WorkspaceIr};

#[derive(derive_getters::Getters)]
pub(crate) struct ApplyEvidence {
    workspace: WorkspaceIr,
    findings: BTreeMap<String, Vec<Box<dyn Finding>>>,
}

/// Keep crate-local anchors attached to their graph and apply exceptions centrally.
#[tracing::instrument(level = "debug", skip(session, filter), err(level = "warn"))]
#[hotpath::measure]
pub(crate) fn collect(
    session: &RuntimeSession,
    filter: &dyn RunFilter,
) -> CordialResult<ApplyEvidence> {
    let config = crate::load_session_config(session);
    let etiquettes: Vec<_> = resolve::resolved_etiquettes(
        session.registered_plugins(),
        session.registered_etiquettes(),
        filter,
        session,
    )
    .into_iter()
    .filter(|etiquette| config.etiquette_may_run(etiquette.configuration_key()))
    .collect();
    if etiquettes.is_empty() {
        return Ok(ApplyEvidence {
            workspace: WorkspaceIr::default(),
            findings: BTreeMap::new(),
        });
    }
    let store = StoreLayout::from_root(
        session.store_root(),
        crate::project_slug_from_path(session.project_root()),
    );
    store.ensure_dirs()?;
    let targets = crate::discover_run_crate_targets(
        session.registered_plugins(),
        session.project_root(),
        session,
        filter,
    )?;
    let loaders = resolve::dedupe_loaders(&etiquettes);
    let enrichers = resolve::dedupe_enrichers(&etiquettes, &loaders);
    let probes = resolve::dedupe_probes(&etiquettes);
    let assessors = resolve::dedupe_assessors(&etiquettes);
    let ids: Vec<_> = etiquettes.iter().map(|etiquette| etiquette.id()).collect();
    let mut timings = HookTimings::default();
    let (workspace, markers) = load::load_and_probe(
        session,
        filter,
        &store,
        &targets,
        load::LoadingHooks::new(
            &loaders,
            &enrichers,
            &probes,
            etiquettes
                .iter()
                .any(|etiquette| etiquette.needs_gated_semantics()),
        ),
        &mut timings,
    )?
    .into_parts();
    let mut findings = BTreeMap::new();
    for target in &targets {
        findings.insert(
            target.crate_name().clone(),
            assess::assess_targets(
                session,
                &store,
                std::slice::from_ref(target),
                &workspace,
                &markers,
                assess::AssessmentHooks::new(&assessors, &ids),
                &mut timings,
            )?,
        );
    }
    Ok(ApplyEvidence {
        workspace,
        findings,
    })
}
