//! Assess each probed target and the workspace as a whole.

use std::collections::HashMap;

use tracing::instrument;

use crate::error::CordialResult;

use crate::hooks::{AssessView, Assessor};
use crate::ir::{CrateView, WorkspaceIr};
use crate::loader::CrateTarget;
use crate::objects::{Finding, Marker};

use crate::store::StoreLayout;

use crate::session::{RuntimeSession, SessionView};

#[instrument(
    level = "debug",
    skip(session, store, targets, workspace, markers_by_crate, assessors),
    err(level = "warn")
)]
pub(super) fn assess_targets(
    session: &RuntimeSession,
    store: &StoreLayout,
    targets: &[CrateTarget],
    workspace: &WorkspaceIr,
    markers_by_crate: &HashMap<String, Vec<Box<dyn Marker>>>,
    assessors: &[&dyn Assessor],
    etiquette_ids: &[&str],
) -> CordialResult<Vec<Box<dyn Finding>>> {
    let mut all_findings: Vec<Box<dyn Finding>> = Vec::new();

    let progress = session
        .progress()
        .bar("Assessing crate findings".to_string(), targets.len() as u64);
    for (index, target) in targets.iter().enumerate() {
        progress.set_message(format!(
            "Assessing {} ({}/{})",
            target.crate_name(),
            index + 1,
            targets.len()
        ));
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
            let relevant: Vec<&dyn Marker> = marker_refs
                .iter()
                .copied()
                .filter(|marker| assessor.consumes().contains(&marker.label()))
                .collect();
            let mut findings = assessor.assess(AssessView::new(&relevant, &crate_view, session))?;
            crate_findings.append(&mut findings);
        }

        let exception_sets =
            crate::exceptions::load_exception_sets(store, etiquette_ids, target.crate_name())?;
        crate::exceptions::warn_stale_exceptions(
            target.crate_name(),
            &crate_findings,
            &exception_sets,
        );
        crate_findings = crate::exceptions::apply_exception_sets(crate_findings, &exception_sets);
        all_findings.extend(crate_findings);
        progress.inc(1);
    }

    progress.finish(format!("Assessed {} crate(s)", targets.len()));
    Ok(all_findings)
}
