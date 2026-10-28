//! Load workspace wrapper coverage from the elicitation hub graph IR.

use tracing::instrument;

use crate::error::CordialResult;
use crate::feature_probe::hub_crate_name;
use crate::hooks::{IrEnricher, Loader};
use crate::ir::{WorkspaceIr, build_wrapper_coverage_from_hub_ir, load_crate_ir_if_missing};
use crate::plugin::{WorkspaceHub, discover_workspace_hub};
use crate::rustdoc::WrapperCoverageMap;
use crate::session::{RunFilter, SessionView};

/// Ensure hub crate IR is loaded, then store wrapper coverage on the workspace IR.
#[instrument(
    level = "debug",
    skip(workspace, session, filter, loaders, enrichers),
    err(level = "warn")
)]
#[hotpath::measure]
pub fn ensure_workspace_wrapper_coverage(
    workspace: &mut WorkspaceIr,
    session: &dyn SessionView,
    filter: &dyn RunFilter,
    loaders: &[&dyn Loader],
    enrichers: &[&dyn IrEnricher],
) -> CordialResult<()> {
    let hub = discover_workspace_hub(session.project_root(), filter)?;
    if hub != WorkspaceHub::Elicitation {
        workspace.set_wrapper_coverage_map(WrapperCoverageMap::new());
        return Ok(());
    }

    let Some(hub_name) = hub_crate_name(hub) else {
        workspace.set_wrapper_coverage_map(WrapperCoverageMap::new());
        return Ok(());
    };

    load_crate_ir_if_missing(workspace, session, hub_name, None, loaders, enrichers)?;
    let map = build_wrapper_coverage_from_hub_ir(workspace, hub_name);

    workspace.set_wrapper_coverage_map(map);
    Ok(())
}
