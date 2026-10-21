//! Load every target and run its probes.

use std::collections::HashMap;

use tracing::instrument;

use crate::error::{CordialError, CordialResult};

use crate::hooks::{EnrichView, IrEnricher, LoadContext, Loader, Probe, ProbeView};
use crate::ir::{CrateIr, CrateView, CrateViewMut, WorkspaceIr};
use crate::loader::{CrateTarget, LoadView, SourceLoadView, SourceLoader};
use crate::objects::Marker;

use crate::HookTimings;
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

#[derive(derive_new::new)]
pub(super) struct LoadingHooks<'a> {
    loaders: &'a [&'a dyn Loader],
    enrichers: &'a [&'a dyn IrEnricher],
    probes: &'a [&'a dyn Probe],
}

#[instrument(
    level = "info",
    skip(session, filter, store, targets, hooks, timings),
    err(level = "warn")
)]
#[hotpath::measure]
pub(super) fn load_and_probe(
    session: &RuntimeSession,
    filter: &dyn RunFilter,
    store: &StoreLayout,
    targets: &[CrateTarget],
    hooks: LoadingHooks<'_>,
    timings: &mut HookTimings,
) -> CordialResult<LoadedWorkspace> {
    crate::loader::reset_source_work();
    let LoadingHooks {
        loaders,
        enrichers,
        probes,
    } = hooks;
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
        let task = session
            .progress()
            .spinner("Preparing wrapper coverage".to_string());
        let result = timings.measure("pipeline", "wrapper_coverage", || {
            crate::rustdoc::ensure_workspace_wrapper_coverage(
                &mut workspace,
                session,
                filter,
                loaders,
                enrichers,
            )
        });
        task.finish("Prepared wrapper coverage".to_string());
        result?;
    }
    #[cfg(not(feature = "impl_coverage"))]
    let _ = filter;

    let progress = session.progress().bar(
        "Loading, enriching, and probing crates".to_string(),
        targets.len() as u64 * (loaders.len() + enrichers.len() + probes.len() + 1) as u64,
    );
    for target in targets {
        let mut crate_ir = CrateIr::new(target.crate_name());

        for loader in loaders {
            progress.set_message(format!(
                "Loading {} with {}",
                target.crate_name(),
                loader.id()
            ));
            let view = timings.measure("load", loader.id(), || {
                loader.load(LoadContext::new(session, target))
            })?;
            if view.loader_id() == SourceLoader::ID
                && let Some(source) = view.as_any().downcast_ref::<SourceLoadView>()
            {
                progress.set_message(format!("Building source IR for {}", target.crate_name()));
                timings.measure("pipeline", "source_ir", || {
                    source.populate_ir(&mut crate_ir)
                })?;
            }
            #[cfg(feature = "rustdoc")]
            if view.loader_id() == crate::RustdocLoader::ID
                && let Some(rustdoc) = view.as_any().downcast_ref::<crate::RustdocLoadView>()
            {
                progress.set_message(format!("Building rustdoc IR for {}", target.crate_name()));
                timings.measure("pipeline", "rustdoc_ir", || {
                    rustdoc.populate_ir(&mut crate_ir)
                })?;
            }
            load_views
                .entry(format!("{}:{}", target.crate_name(), loader.id()))
                .or_insert(view);
            progress.inc(1);
        }

        if !workspace.crates().contains_key(target.crate_name()) {
            workspace.insert_crate(crate_ir);
        }

        for enricher in enrichers {
            progress.set_message(format!(
                "Enriching {} with {}",
                target.crate_name(),
                enricher.id()
            ));
            let load = select_load_view(*enricher, &load_views, target.crate_name())?;
            let mut view = CrateViewMut::new(&mut workspace, target.crate_name().clone());
            timings.measure("enrich", enricher.id(), || {
                enricher.enrich(EnrichView::new(&mut view, load, session))
            })?;
            progress.inc(1);
        }

        if let Some(source) = load_views
            .get(&format!("{}:{}", target.crate_name(), SourceLoader::ID))
            .and_then(|view| view.as_any().downcast_ref::<SourceLoadView>())
        {
            source.release_parsed();
        }

        progress.set_message(format!("Caching IR for {}", target.crate_name()));
        timings.measure("pipeline", "cache_ir", || {
            let cached = workspace
                .crate_ir(target.crate_name())
                .ok_or_else(|| CordialError::invariant("crate ir must exist"))?;
            cached.write_cache(&store.ir_cache_path(target.crate_name()))?;
            let digest = crate::cache_digest::IrCacheDigest::compute(
                target,
                &enricher_id_refs,
                &load_views,
            )?;
            digest.write(&crate::cache_digest::IrCacheDigest::cache_path(
                &store.cache_dir(),
                target.crate_name(),
            ))
        })?;
        progress.inc(1);

        let crate_view = CrateView::new(&workspace, target.crate_name().clone());
        for probe in probes {
            progress.set_message(format!(
                "Probing {} with {}",
                target.crate_name(),
                probe.id()
            ));
            let mut found = timings.measure("probe", probe.id(), || {
                probe.probe(ProbeView::new(&crate_view, session))
            })?;
            markers_by_crate
                .entry(target.crate_name().clone())
                .or_default()
                .append(&mut found);
            progress.inc(1);
        }
    }
    progress.finish(format!("Analyzed {} crate(s)", targets.len()));

    crate::loader::write_source_work()?;
    Ok(LoadedWorkspace::new(workspace, markers_by_crate))
}
