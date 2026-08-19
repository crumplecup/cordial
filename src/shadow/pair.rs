//! Cross-crate upstream ↔ shadow pair report construction.

use tracing::instrument;

use crate::error::CordialResult;
use crate::hooks::{IrEnricher, Loader};
use crate::ir::{WorkspaceIr, load_crate_ir_if_missing};
use crate::plugin::discover_active_shadow_pairs;
use crate::session::{RunFilter, SessionView};

use super::ShadowReport;
use super::ir::{
    build_shadow_pair_report_from_workspace_ir, materialize_cross_crate_shadow_mirrors,
};

/// Ensure IR for active shadow pairs is loaded and cross-crate mirrors materialized.
#[instrument(
    level = "debug",
    skip(workspace, session, filter, loaders, enrichers),
    err(level = "warn")
)]
pub fn preload_shadow_pair_crates(
    workspace: &mut WorkspaceIr,
    session: &dyn SessionView,
    filter: &dyn RunFilter,
    loaders: &[&dyn Loader],
    enrichers: &[&dyn IrEnricher],
) -> CordialResult<()> {
    for pair in discover_active_shadow_pairs(session.project_root(), filter)? {
        load_crate_ir_if_missing(workspace, session, &pair.shadow, None, loaders, enrichers)?;
        load_crate_ir_if_missing(
            workspace,
            session,
            &pair.upstream,
            Some(pair.shadow.as_str()),
            loaders,
            enrichers,
        )?;
        materialize_cross_crate_shadow_mirrors(workspace, &pair.upstream, &pair.shadow)?;
    }
    Ok(())
}

/// Build shadow mirror reports for every active tracked pair in the workspace.
#[instrument(level = "info", skip(workspace, session, filter), err(level = "warn"))]
pub fn load_workspace_shadow_reports(
    workspace: &WorkspaceIr,
    session: &dyn SessionView,
    filter: &dyn RunFilter,
) -> CordialResult<Vec<(String, String, ShadowReport)>> {
    let pairs = discover_active_shadow_pairs(session.project_root(), filter)?;
    let mut reports = Vec::new();
    for pair in pairs {
        let report =
            build_shadow_pair_report_from_workspace(workspace, &pair.upstream, &pair.shadow)?;
        reports.push((pair.upstream, pair.shadow, report));
    }
    Ok(reports)
}

/// Build one upstream ↔ shadow mirror report from workspace graph IR.
#[instrument(level = "debug", skip(workspace), err(level = "warn"))]
pub fn build_shadow_pair_report_from_workspace(
    workspace: &WorkspaceIr,
    upstream: &str,
    shadow: &str,
) -> CordialResult<ShadowReport> {
    build_shadow_pair_report_from_workspace_ir(workspace, upstream, shadow)
}
