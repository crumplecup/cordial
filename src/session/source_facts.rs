//! Workspace source evidence assembled once per run and retained in the IR.

use crate::error::CordialResult;
use crate::hooks::IrEnricher;
use crate::ir::WorkspaceIr;
use crate::loader::SourceLoadView;
use crate::session::SessionView;

#[cfg(any(feature = "impl_coverage", feature = "amenable_std"))]
#[tracing::instrument(level = "debug", skip(workspace, session, enrichers, probe_ids))]
pub(crate) fn prepare_workspace_proof_harness(
    workspace: &mut WorkspaceIr,
    session: &dyn SessionView,
    enrichers: &[&dyn IrEnricher],
    probe_ids: &[&str],
) -> CordialResult<()> {
    let needed_by_enricher = {
        #[cfg(feature = "impl_coverage")]
        {
            enrichers
                .iter()
                .any(|enricher| enricher.id() == crate::enricher::ProofHarnessEnricher::ID)
        }
        #[cfg(not(feature = "impl_coverage"))]
        {
            let _ = enrichers;
            false
        }
    };
    let needed_by_probe = probe_ids
        .iter()
        .any(|id| *id == "amenable-std-scope" || id.starts_with("amenable-ext-"));
    if (needed_by_enricher || needed_by_probe) && workspace.proof_harness().is_none() {
        let source = SourceLoadView::for_standalone_scan();
        let filter = crate::NamedRunFilter::all_etiquettes();
        workspace.set_proof_harness(
            crate::proof_harness::load_workspace_proof_harness_with_source(
                session.project_root(),
                &filter,
                &source,
            )?,
        );
    }
    Ok(())
}

#[cfg(any(feature = "derives", feature = "antipatterns"))]
#[tracing::instrument(level = "debug", skip(workspace, source, session, enrichers))]
pub(crate) fn prepare_workspace_source_facts(
    workspace: &mut WorkspaceIr,
    source: &SourceLoadView,
    session: &dyn SessionView,
    enrichers: &[&dyn IrEnricher],
) -> CordialResult<()> {
    #[cfg(feature = "antipatterns")]
    if enrichers.iter().any(|enricher| {
        enricher.id() == crate::etiquettes::antipatterns::AntipatternInventoryEnricher::ID
    }) && workspace.antipattern_facts().is_none()
    {
        workspace.set_antipattern_facts(
            crate::etiquettes::antipatterns::AntipatternWorkspaceFacts::load(
                session.project_root(),
                session.store_root(),
            )?,
        );
    }

    #[cfg(feature = "derives")]
    {
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
            workspace.set_path_inclusions(source.project_path_inclusions()?);
        }
    }

    #[cfg(feature = "tracing")]
    if enrichers
        .iter()
        .any(|enricher| enricher.id() == crate::etiquettes::tracing::FunctionInventoryEnricher::ID)
        && workspace.call_graph().is_none()
    {
        let config = crate::config::load_session_config(session);
        workspace.refresh_tracing_call_graph(config.tracing())?;
    }
    #[cfg(not(feature = "derives"))]
    let _ = source;
    #[cfg(not(any(feature = "antipatterns", feature = "tracing")))]
    let _ = session;
    Ok(())
}
