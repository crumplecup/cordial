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

#[cfg(any(
    feature = "derives",
    feature = "antipatterns",
    all(
        feature = "verus_ir",
        any(
            feature = "panics",
            feature = "proof_patterns",
            feature = "verus_warnings"
        )
    )
))]
#[tracing::instrument(level = "debug", skip(workspace, source, session, enrichers))]
pub(crate) fn prepare_workspace_source_facts(
    workspace: &mut WorkspaceIr,
    source: &SourceLoadView,
    session: &dyn SessionView,
    enrichers: &[&dyn IrEnricher],
) -> CordialResult<()> {
    #[cfg(all(
        feature = "verus_ir",
        any(
            feature = "panics",
            feature = "proof_patterns",
            feature = "verus_warnings"
        )
    ))]
    {
        let needs_verus = enrichers.iter().any(|enricher| {
            #[cfg(feature = "panics")]
            if enricher.id() == crate::etiquettes::panics::PanicInventoryEnricher::ID {
                return true;
            }
            #[cfg(feature = "proof_patterns")]
            if enricher.id() == crate::etiquettes::ProofPatternInventoryEnricher::ID {
                return true;
            }
            #[cfg(feature = "verus_warnings")]
            if enricher.id() == crate::etiquettes::verus_warnings::VerusWarningInventoryEnricher::ID
            {
                return true;
            }
            false
        });
        if needs_verus && workspace.verus_facts(source.crate_name()).is_none() {
            let crate_root = source
                .src_root()
                .parent()
                .unwrap_or_else(|| session.project_root());
            workspace.set_verus_facts(
                source.crate_name().to_string(),
                crate::verus_ir::scan_crate_verus_ir_with_source(crate_root, source)?,
            );
        }
    }
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
            workspace.set_path_inclusions(
                crate::etiquettes::derives::workspace_path_inclusions_with_source(
                    session.project_root(),
                    source,
                ),
            );
        }
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
    #[cfg(not(feature = "derives"))]
    let _ = source;
    Ok(())
}
