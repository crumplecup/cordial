//! Session pipeline: load, probe, assess, render.

use std::collections::HashMap;

use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::etiquette::Etiquette;
use crate::hooks::{
    AssessView, Assessor, EnrichView, IrEnricher, LoadContext, Loader, Probe, ProbeView,
    RenderView, Reporter, WorkspaceAssessView,
};
use crate::ir::{CrateIr, CrateView, CrateViewMut, WorkspaceIr};
use crate::loader::{CrateTarget, LoadView, SourceLoadView, SourceLoader};
#[cfg(any(
    feature = "homecoming_std",
    feature = "amenable_std",
    feature = "elicitation"
))]
use crate::objects::TextArtifact;
use crate::objects::{Artifact, Finding, Marker};
use crate::plugin::{Plugin, PluginCategory, plugins_in_category, selected_plugins};
#[cfg(feature = "quality")]
use crate::reporter::QualityReportReporter;
use crate::reporter::RollupReporter;
#[cfg(any(
    feature = "homecoming_std",
    feature = "amenable_std",
    feature = "elicitation"
))]
use crate::reporter::{build_coverage_summary, render_coverage_summary_markdown};
use crate::store::StoreLayout;
#[cfg(feature = "rustdoc")]
use crate::{RustdocLoadView, RustdocLoader};

use super::resolve::{
    dedupe_assessors, dedupe_enrichers, dedupe_loaders, dedupe_probes, dedupe_reporters,
    dedupe_workspace_assessors, resolved_etiquettes, select_load_view,
};
use super::{RunFilter, RunOutcome, RuntimeSession};

#[cfg(any(
    feature = "homecoming_std",
    feature = "amenable_std",
    feature = "elicitation"
))]
#[instrument(level = "info", skip(plugins, filter, etiquettes))]
fn run_includes_coverage(
    plugins: &[&'static dyn Plugin],
    filter: &dyn RunFilter,
    etiquettes: &[&'static dyn Etiquette],
) -> bool {
    !plugins_in_category(
        &selected_plugins(plugins, filter.plugins()),
        PluginCategory::Coverage,
    )
    .is_empty()
        || etiquettes.iter().any(|etiquette| etiquette.is_coverage())
}

#[instrument(level = "info", skip(plugins, filter, etiquettes))]
#[cfg(feature = "quality")]
fn run_includes_quality(
    plugins: &[&'static dyn Plugin],
    filter: &dyn RunFilter,
    etiquettes: &[&'static dyn Etiquette],
) -> bool {
    let active = selected_plugins(plugins, filter.plugins());
    !plugins_in_category(&active, PluginCategory::Quality).is_empty()
        || !plugins_in_category(&active, PluginCategory::ErrorHandling).is_empty()
        || etiquettes.iter().any(|etiquette| !etiquette.is_coverage())
}

struct ConcreteRunOutcome {
    findings: Vec<Box<dyn Finding>>,
    artifacts: Vec<Box<dyn Artifact>>,
}

impl RunOutcome for ConcreteRunOutcome {
    #[instrument(level = "trace", skip(self))]
    fn findings(&self) -> Box<dyn Iterator<Item = &dyn Finding> + '_> {
        Box::new(
            self.findings
                .iter()
                .map(|finding| finding.as_ref() as &dyn Finding),
        )
    }

    #[instrument(level = "trace", skip(self))]
    fn artifacts(&self) -> Box<dyn Iterator<Item = &dyn Artifact> + '_> {
        Box::new(
            self.artifacts
                .iter()
                .map(|artifact| artifact.as_ref() as &dyn Artifact),
        )
    }
}

#[instrument(level = "debug")]
fn empty_outcome() -> Box<dyn RunOutcome> {
    Box::new(ConcreteRunOutcome {
        findings: Vec::new(),
        artifacts: Vec::new(),
    })
}

#[instrument(level = "info", skip(session, filter), err(level = "warn"))]
pub(super) fn run_session(
    session: &RuntimeSession,
    filter: &dyn RunFilter,
) -> CordialResult<Box<dyn RunOutcome>> {
    let store = StoreLayout::from_root(
        &session.store_root,
        crate::store::project_slug_from_path(&session.project_root),
    );
    store.ensure_dirs()?;

    let etiquettes = resolved_etiquettes(&session.plugins, &session.etiquettes, filter);
    let config = crate::load_session_config(session);
    let etiquettes: Vec<&'static dyn Etiquette> = etiquettes
        .into_iter()
        .filter(|etiquette| config.etiquette_enabled(etiquette.id()))
        .collect();
    if etiquettes.is_empty() {
        return Ok(empty_outcome());
    }

    let targets = crate::targets::discover_run_crate_targets(
        &session.plugins,
        &session.project_root,
        session,
        filter,
    )?;
    if targets.is_empty() {
        return Ok(empty_outcome());
    }

    let loaders = dedupe_loaders(&etiquettes);
    let enrichers = dedupe_enrichers(&etiquettes, &loaders);
    let probes = dedupe_probes(&etiquettes);
    let assessors = dedupe_assessors(&etiquettes);
    let workspace_assessors = dedupe_workspace_assessors(&etiquettes);
    let reporters = dedupe_reporters(&etiquettes);

    let loaded = load_and_probe(
        session, filter, &store, &targets, &loaders, &enrichers, &probes,
    )?;
    #[cfg(feature = "shadow")]
    let mut workspace = loaded.workspace;
    #[cfg(not(feature = "shadow"))]
    let workspace = loaded.workspace;

    let etiquette_ids: Vec<&str> = etiquettes.iter().map(|etiquette| etiquette.id()).collect();
    let mut all_findings = assess_targets(
        session,
        &store,
        &targets,
        &workspace,
        &loaded.markers_by_crate,
        &assessors,
        &etiquette_ids,
    )?;

    #[cfg(feature = "shadow")]
    {
        crate::shadow::preload_shadow_pair_crates(
            &mut workspace,
            session,
            filter,
            &loaders,
            &enrichers,
        )?;
    }

    for assessor in &workspace_assessors {
        all_findings.extend(assessor.assess(WorkspaceAssessView {
            workspace: &workspace,
            session,
            filter,
        })?);
    }

    let all_artifacts = render_and_write(
        session,
        filter,
        RenderPass {
            store: &store,
            targets: &targets,
            workspace: &workspace,
            etiquettes: &etiquettes,
            etiquette_ids: &etiquette_ids,
            reporters: &reporters,
            findings: &all_findings,
        },
    )?;

    Ok(Box::new(ConcreteRunOutcome {
        findings: all_findings,
        artifacts: all_artifacts,
    }))
}

struct LoadedWorkspace {
    workspace: WorkspaceIr,
    markers_by_crate: HashMap<String, Vec<Box<dyn Marker>>>,
}

#[instrument(
    level = "info",
    skip(session, filter, store, targets, loaders, enrichers, probes),
    err(level = "warn")
)]
fn load_and_probe(
    session: &RuntimeSession,
    filter: &dyn RunFilter,
    store: &StoreLayout,
    targets: &[CrateTarget],
    loaders: &[&'static dyn Loader],
    enrichers: &[&'static dyn IrEnricher],
    probes: &[&'static dyn Probe],
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

    for target in targets {
        let mut crate_ir = CrateIr::new(&target.crate_name);

        for loader in loaders {
            let view = loader.load(LoadContext { session, target })?;
            if view.loader_id() == SourceLoader::ID
                && let Some(source) = view.as_any().downcast_ref::<SourceLoadView>()
            {
                source.populate_ir(&mut crate_ir)?;
            }
            #[cfg(feature = "rustdoc")]
            if view.loader_id() == RustdocLoader::ID
                && let Some(rustdoc) = view.as_any().downcast_ref::<RustdocLoadView>()
            {
                rustdoc.populate_ir(&mut crate_ir)?;
            }
            load_views
                .entry(format!("{}:{}", target.crate_name, loader.id()))
                .or_insert(view);
        }

        if !workspace.crates.contains_key(&target.crate_name) {
            workspace.insert_crate(crate_ir);
        }

        for enricher in enrichers {
            let load = select_load_view(*enricher, &load_views, &target.crate_name)?;
            let mut view = CrateViewMut {
                workspace: &mut workspace,
                crate_name: target.crate_name.clone(),
            };
            enricher.enrich(EnrichView {
                ir: &mut view,
                load,
                session,
            })?;
        }

        let cached = workspace
            .crate_ir(&target.crate_name)
            .ok_or_else(|| CordialError::invariant("crate ir must exist"))?;
        cached.write_cache(&store.ir_cache_path(&target.crate_name))?;
        let digest =
            crate::cache_digest::IrCacheDigest::compute(target, &enricher_id_refs, &load_views)?;
        digest.write(&crate::cache_digest::IrCacheDigest::cache_path(
            &store.cache_dir(),
            &target.crate_name,
        ))?;

        let crate_view = CrateView {
            workspace: &workspace,
            crate_name: target.crate_name.clone(),
        };
        for probe in probes {
            let mut found = probe.probe(ProbeView {
                ir: &crate_view,
                session,
            })?;
            markers_by_crate
                .entry(target.crate_name.clone())
                .or_default()
                .append(&mut found);
        }
    }

    Ok(LoadedWorkspace {
        workspace,
        markers_by_crate,
    })
}

#[instrument(
    level = "debug",
    skip(session, store, targets, workspace, markers_by_crate, assessors),
    err(level = "warn")
)]
fn assess_targets(
    session: &RuntimeSession,
    store: &StoreLayout,
    targets: &[CrateTarget],
    workspace: &WorkspaceIr,
    markers_by_crate: &HashMap<String, Vec<Box<dyn Marker>>>,
    assessors: &[&'static dyn Assessor],
    etiquette_ids: &[&str],
) -> CordialResult<Vec<Box<dyn Finding>>> {
    let mut all_findings: Vec<Box<dyn Finding>> = Vec::new();

    for target in targets {
        let markers = markers_by_crate
            .get(&target.crate_name)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let marker_refs: Vec<&dyn Marker> = markers
            .iter()
            .map(|marker| marker.as_ref() as &dyn Marker)
            .collect();
        let crate_view = CrateView {
            workspace,
            crate_name: target.crate_name.clone(),
        };

        let mut crate_findings: Vec<Box<dyn Finding>> = Vec::new();
        for assessor in assessors {
            let relevant: Vec<&dyn Marker> = marker_refs
                .iter()
                .copied()
                .filter(|marker| assessor.consumes().contains(&marker.label()))
                .collect();
            let mut findings = assessor.assess(AssessView {
                markers: &relevant,
                ir: &crate_view,
                session,
            })?;
            crate_findings.append(&mut findings);
        }

        let exception_sets =
            crate::exceptions::load_exception_sets(store, etiquette_ids, &target.crate_name)?;
        crate_findings = crate::exceptions::apply_exception_sets(crate_findings, &exception_sets);
        all_findings.extend(crate_findings);
    }

    Ok(all_findings)
}

struct RenderPass<'a> {
    store: &'a StoreLayout,
    targets: &'a [CrateTarget],
    workspace: &'a WorkspaceIr,
    etiquettes: &'a [&'static dyn Etiquette],
    etiquette_ids: &'a [&'a str],
    reporters: &'a [&'static dyn Reporter],
    findings: &'a [Box<dyn Finding>],
}

#[instrument(level = "debug", skip(session, filter, pass), err(level = "warn"))]
fn render_and_write(
    session: &RuntimeSession,
    filter: &dyn RunFilter,
    pass: RenderPass<'_>,
) -> CordialResult<Vec<Box<dyn Artifact>>> {
    let RenderPass {
        store,
        targets,
        workspace,
        etiquettes,
        etiquette_ids,
        reporters,
        findings: all_findings,
    } = pass;
    let primary_name = targets
        .first()
        .map(|target| target.crate_name.clone())
        .ok_or_else(|| CordialError::invariant("workspace missing crate targets"))?;
    let crate_view = CrateView {
        workspace,
        crate_name: primary_name,
    };

    let finding_refs: Vec<&dyn Finding> = all_findings
        .iter()
        .map(|finding| finding.as_ref() as &dyn Finding)
        .collect();

    let mut all_artifacts: Vec<Box<dyn Artifact>> = Vec::new();
    for reporter in reporters {
        let mut artifacts = reporter.render(RenderView {
            findings: &finding_refs,
            ir: &crate_view,
            session,
        })?;
        all_artifacts.append(&mut artifacts);
    }

    let rollup = RollupReporter;
    let mut rollup_artifacts = rollup.render(RenderView {
        findings: &finding_refs,
        ir: &crate_view,
        session,
    })?;
    all_artifacts.append(&mut rollup_artifacts);

    #[cfg(feature = "quality")]
    let includes_quality = run_includes_quality(&session.plugins, filter, etiquettes);
    #[cfg(not(feature = "quality"))]
    let includes_quality = false;

    #[cfg(feature = "quality")]
    if includes_quality {
        let quality_report = QualityReportReporter;
        let mut quality_artifacts = quality_report.render(RenderView {
            findings: &finding_refs,
            ir: &crate_view,
            session,
        })?;
        all_artifacts.append(&mut quality_artifacts);
    }

    #[cfg(any(
        feature = "homecoming_std",
        feature = "amenable_std",
        feature = "elicitation"
    ))]
    if run_includes_coverage(&session.plugins, filter, etiquettes) {
        let summary = build_coverage_summary(
            &session.plugins,
            etiquette_ids,
            filter,
            session,
            &finding_refs,
            workspace,
        )?;
        let body = render_coverage_summary_markdown(&summary)?;
        let summary_name = if includes_quality {
            "coverage-summary.md"
        } else {
            "summary.md"
        };
        all_artifacts.push(Box::new(TextArtifact {
            name: summary_name.to_string(),
            media_type: "text/markdown".to_string(),
            body,
        }));
        all_artifacts.extend(summary.extra_artifacts);
    }
    #[cfg(not(any(
        feature = "homecoming_std",
        feature = "amenable_std",
        feature = "elicitation"
    )))]
    let _ = (filter, etiquettes, etiquette_ids);

    for artifact in &all_artifacts {
        let path = store.findings_dir().join(artifact.name());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&path)?;
        artifact.write_to(&mut file)?;
    }

    Ok(all_artifacts)
}
