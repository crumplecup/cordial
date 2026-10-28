//! Workspace source evidence assembled once per run and retained in the IR.

use crate::hooks::IrEnricher;
use crate::ir::WorkspaceIr;
use crate::loader::SourceLoadView;
use crate::session::SessionView;

#[tracing::instrument(level = "debug", skip(workspace, source, session, enrichers))]
pub(crate) fn prepare_workspace_source_facts(
    workspace: &mut WorkspaceIr,
    source: &SourceLoadView,
    session: &dyn SessionView,
    enrichers: &[&dyn IrEnricher],
) {
    let needs_path_facts = enrichers.iter().any(|enricher| {
        enricher.id() == crate::etiquettes::derives::DeriveInventoryEnricher::ID || {
            #[cfg(feature = "tracing")]
            {
                enricher.id() == crate::etiquettes::tracing::FunctionInventoryEnricher::ID
            }
            #[cfg(not(feature = "tracing"))]
            {
                false
            }
        }
    });
    if needs_path_facts && workspace.path_inclusions().is_none() {
        workspace.set_path_inclusions(
            crate::etiquettes::derives::workspace_path_inclusions_with_source(
                session.project_root(),
                source,
            ),
        );
    }

    #[cfg(feature = "tracing")]
    if enrichers
        .iter()
        .any(|enricher| enricher.id() == crate::etiquettes::tracing::FunctionInventoryEnricher::ID)
        && workspace.call_graph().is_none()
        && let Some(path_facts) = workspace.path_inclusions()
    {
        let config = crate::config::load_session_config(session);
        let call_graph = crate::etiquettes::tracing::workspace_call_graph_with_source(
            config.tracing(),
            path_facts,
            source,
        );
        workspace.set_call_graph(call_graph);
    }
}
