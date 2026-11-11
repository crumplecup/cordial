//! Assess each probed target and the workspace as a whole.

use std::collections::HashMap;

use tracing::instrument;

use crate::error::CordialResult;

use crate::hooks::{AssessView, Assessor};
use crate::ir::{CrateView, WorkspaceIr};
use crate::loader::CrateTarget;
use crate::objects::{Finding, Marker};

use crate::HookTimings;
use crate::store::StoreLayout;

use crate::session::{RuntimeSession, SessionView};

#[derive(derive_new::new)]
pub(super) struct AssessmentHooks<'a> {
    assessors: &'a [&'a dyn Assessor],
    etiquette_ids: &'a [&'a str],
}

#[instrument(
    level = "debug",
    skip(session, store, targets, workspace, markers_by_crate, hooks, timings),
    err(level = "warn")
)]
#[hotpath::measure]
pub(super) fn assess_targets(
    session: &RuntimeSession,
    store: &StoreLayout,
    targets: &[CrateTarget],
    workspace: &WorkspaceIr,
    markers_by_crate: &HashMap<String, Vec<Box<dyn Marker>>>,
    hooks: AssessmentHooks<'_>,
    timings: &mut HookTimings,
) -> CordialResult<Vec<Box<dyn Finding>>> {
    let AssessmentHooks {
        assessors,
        etiquette_ids,
    } = hooks;
    let mut all_findings: Vec<Box<dyn Finding>> = Vec::new();
    let config = crate::load_session_config(session);

    let progress = session.progress().bar(
        "Assessing crate findings".to_string(),
        targets.len() as u64 * (assessors.len() + 1) as u64,
    );
    for target in targets {
        let markers = markers_by_crate
            .get(target.crate_name())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let marker_refs: Vec<&dyn Marker> = markers
            .iter()
            .map(|marker| marker.as_ref() as &dyn Marker)
            .collect();
        let crate_view = CrateView::new(workspace, target.crate_name().clone());

        let mut crate_findings: Vec<Box<dyn Finding>> = Vec::new();
        for assessor in assessors {
            progress.set_message(format!(
                "Assessing {} with {}",
                target.crate_name(),
                assessor.id()
            ));
            let relevant: Vec<&dyn Marker> = marker_refs
                .iter()
                .copied()
                .filter(|marker| assessor.consumes().contains(&marker.label()))
                .collect();
            let mut findings = timings.measure("assess", assessor.id(), || {
                assessor.assess(AssessView::new(&relevant, &crate_view, session))
            })?;
            crate_findings.append(&mut findings);
            progress.inc(1);
        }

        let location = crate::LintLocation::new(
            session.project_root(),
            Some(target.crate_root()),
            target.crate_name(),
            None,
        );
        crate_findings.retain(|finding| {
            config.finding_enabled(finding.as_ref(), location, Some(&crate_view))
        });
        progress.set_message(format!("Applying exceptions for {}", target.crate_name()));
        crate_findings = timings.measure("pipeline", "exceptions", || {
            let exception_sets =
                crate::exceptions::load_exception_sets(store, etiquette_ids, target.crate_name())?;
            #[cfg(feature = "_etiquette_plugins")]
            let stale = if etiquette_ids.contains(&"stale_exceptions") {
                stale_findings(target.crate_name(), &crate_findings, &exception_sets)
            } else {
                Vec::new()
            };
            #[cfg(not(feature = "_etiquette_plugins"))]
            let stale: Vec<Box<dyn Finding>> = Vec::new();
            let mut applied =
                crate::exceptions::apply_exception_sets(crate_findings, &exception_sets);
            applied.extend(stale);
            Ok(applied)
        })?;
        all_findings.extend(crate_findings);
        progress.inc(1);
    }

    progress.finish(format!("Assessed {} crate(s)", targets.len()));
    Ok(all_findings)
}

#[cfg(feature = "_etiquette_plugins")]
/// One open finding per exception row that matches nothing in `findings`.
///
/// Raised only when the stale-exceptions etiquette is part of the run.
#[instrument(level = "debug", skip(findings, sets))]
fn stale_findings(
    crate_name: &str,
    findings: &[Box<dyn Finding>],
    sets: &HashMap<String, crate::exceptions::ExceptionSet>,
) -> Vec<Box<dyn Finding>> {
    let refs: Vec<&dyn Finding> = findings.iter().map(Box::as_ref).collect();
    crate::exceptions::stale_exceptions(&refs, sets)
        .into_iter()
        .map(|stale| {
            Box::new(crate::etiquettes::StaleExceptionFinding::new(
                crate_name, stale,
            )) as Box<dyn Finding>
        })
        .collect()
}
