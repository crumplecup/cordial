//! Load every target and run its probes.

use std::collections::HashMap;

use tracing::instrument;

use crate::error::{CordialError, CordialResult};

use crate::hooks::{EnrichView, IrEnricher, LoadContext, Loader, Probe, ProbeView};
use crate::ir::{CrateIr, CrateView, CrateViewMut, WorkspaceIr};
use crate::loader::{CrateTarget, LoadView, SourceLoadView, SourceLoader};
use crate::objects::Marker;

use crate::store::StoreLayout;

use crate::session::resolve::select_load_view;
use crate::session::{RunFilter, RuntimeSession, SessionView};

/// The assembled workspace IR and the probe markers found per crate.
#[derive(derive_new::new)]
pub(super) struct LoadedWorkspace {
    workspace: WorkspaceIr,
    markers_by_crate: HashMap<String, Vec<Box<dyn Marker>>>,
}

impl LoadedWorkspace {
    /// Split into the workspace IR and the per-crate markers.
    #[instrument(level = "debug", skip(self))]
    pub(super) fn into_parts(self) -> (WorkspaceIr, HashMap<String, Vec<Box<dyn Marker>>>) {
        (self.workspace, self.markers_by_crate)
    }
}

#[instrument(
    level = "info",
    skip(session, filter, store, targets, loaders, enrichers, probes),
    err(level = "warn")
)]
pub(super) fn load_and_probe(
    session: &RuntimeSession,
    filter: &dyn RunFilter,
    store: &StoreLayout,
    targets: &[CrateTarget],
    loaders: &[&dyn Loader],
    enrichers: &[&dyn IrEnricher],
    probes: &[&dyn Probe],
) -> CordialResult<LoadedWorkspace> {
    let mut workspace = WorkspaceIr::default();
    let mut load_views: HashMap<String, Box<dyn LoadView>> = HashMap::new();
    let mut markers_by_crate: HashMap<String, Vec<Box<dyn Marker>>> = HashMap::new();
    let enricher_ids: Vec<String> = enrichers
        .iter()
        .map(|enricher| enricher.id().to_string())
        .collect();
    let enricher_id_refs: Vec<&str> = enricher_ids.iter().map(String::as_str).collect();

    #[cfg(feature = "impl_coverage")]
    if enrichers
        .iter()
        .any(|enricher| enricher.id() == crate::enricher::WrapperCoverageEnricher::ID)
    {
        crate::rustdoc::ensure_workspace_wrapper_coverage(
            &mut workspace,
            session,
            filter,
            loaders,
            enrichers,
        )?;
    }
    #[cfg(not(feature = "impl_coverage"))]
    let _ = filter;

    let progress = session.progress().bar(
        "Loading, enriching, and probing crates".to_string(),
        targets.len() as u64,
    );
    for (index, target) in targets.iter().enumerate() {
        progress.set_message(format!(
            "Analyzing {} ({}/{})",
            target.crate_name(),
            index + 1,
            targets.len()
        ));
        let mut crate_ir = CrateIr::new(target.crate_name());

        for loader in loaders {
            let view = loader.load(LoadContext::new(session, target))?;
            if view.loader_id() == SourceLoader::ID
                && let Some(source) = view.as_any().downcast_ref::<SourceLoadView>()
            {
                source.populate_ir(&mut crate_ir)?;
            }
            #[cfg(feature = "rustdoc")]
            if view.loader_id() == crate::RustdocLoader::ID
                && let Some(rustdoc) = view.as_any().downcast_ref::<crate::RustdocLoadView>()
            {
                rustdoc.populate_ir(&mut crate_ir)?;
            }
            load_views
                .entry(format!("{}:{}", target.crate_name(), loader.id()))
                .or_insert(view);
        }

        if !workspace.crates().contains_key(target.crate_name()) {
            workspace.insert_crate(crate_ir);
        }

        for enricher in enrichers {
            let load = select_load_view(*enricher, &load_views, target.crate_name())?;
            let mut view = CrateViewMut::new(&mut workspace, target.crate_name().clone());
            enricher.enrich(EnrichView::new(&mut view, load, session))?;
        }

        let cached = workspace
            .crate_ir(target.crate_name())
            .ok_or_else(|| CordialError::invariant("crate ir must exist"))?;
        cached.write_cache(&store.ir_cache_path(target.crate_name()))?;
        let digest =
            crate::cache_digest::IrCacheDigest::compute(target, &enricher_id_refs, &load_views)?;
        digest.write(&crate::cache_digest::IrCacheDigest::cache_path(
            &store.cache_dir(),
            target.crate_name(),
        ))?;

        let crate_view = CrateView::new(&workspace, target.crate_name().clone());
        for probe in probes {
            let mut found = probe.probe(ProbeView::new(&crate_view, session))?;
            markers_by_crate
                .entry(target.crate_name().clone())
                .or_default()
                .append(&mut found);
        }
        progress.inc(1);
    }
    progress.finish(format!("Analyzed {} crate(s)", targets.len()));

    Ok(LoadedWorkspace::new(workspace, markers_by_crate))
}
