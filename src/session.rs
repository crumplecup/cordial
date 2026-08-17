use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use tracing::instrument;

use crate::enricher::{PathIndexEnricher, SynDocLinkEnricher};
use crate::error::{CordialError, CordialResult};
use crate::etiquette::Etiquette;
use crate::hooks::{Assessor, IrEnricher, Loader, Probe, Reporter, WorkspaceAssessor};
use crate::ir::{CrateIr, CrateView, CrateViewMut, WorkspaceIr};
use crate::loader::{LoadView, SourceLoadView, SourceLoader};
#[cfg(any(
    feature = "homecoming_std",
    feature = "amenable_std",
    feature = "elicitation"
))]
use crate::objects::TextArtifact;
use crate::objects::{Artifact, Finding, Marker};
use crate::plugin::{
    Plugin, PluginCategory, etiquettes_from_plugins, plugins_in_category, selected_plugins,
};
#[cfg(feature = "quality")]
use crate::reporter::QualityReportReporter;
use crate::reporter::RollupReporter;
#[cfg(any(
    feature = "homecoming_std",
    feature = "amenable_std",
    feature = "elicitation"
))]
use crate::reporter::{build_coverage_summary, render_coverage_summary_markdown};
#[cfg(feature = "rustdoc")]
use crate::{RustdocLoadView, RustdocLoader, enricher::RustdocStructureEnricher};

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

/// Read-only session context passed to hooks.
pub trait SessionView: Send + Sync {
    fn project_root(&self) -> &Path;
    fn store_root(&self) -> &Path;
    /// Store home (`~/.cordial` or `--store-home` / `CORDIAL_HOME`).
    fn store_home(&self) -> &Path;
}

/// Filter for which plugins, etiquettes, and crates to run.
pub trait RunFilter: Send + Sync {
    fn plugins(&self) -> Option<&[&str]> {
        None
    }

    fn etiquettes(&self) -> Option<&[&str]>;
    fn crates(&self) -> Option<&[&str]> {
        None
    }
    fn crate_name(&self) -> Option<&str> {
        None
    }
}

/// Outcome of a session run.
pub trait RunOutcome: Send + Sync {
    fn findings(&self) -> Box<dyn Iterator<Item = &dyn Finding> + '_>;
    fn artifacts(&self) -> Box<dyn Iterator<Item = &dyn Artifact> + '_>;
}

/// Orchestrates plugin and etiquette execution.
pub trait Session: Send + Sync {
    fn register(&mut self, etiquette: &'static dyn Etiquette);
    fn register_plugin(&mut self, plugin: &'static dyn Plugin);
    fn run(&self, filter: &dyn RunFilter) -> CordialResult<Box<dyn RunOutcome>>;
}

/// Builder for a default runtime session.
pub struct SessionBuilder {
    project_root: PathBuf,
    store_home: PathBuf,
    store_root: PathBuf,
    plugins: Vec<&'static dyn Plugin>,
    etiquettes: Vec<&'static dyn Etiquette>,
}

impl SessionBuilder {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        let project_root = project_root.into();
        let slug = crate::store::project_slug_from_path(&project_root);
        let store_home = crate::store::default_store_home();
        let store = crate::store::StoreLayout::from_root(store_home.join(&slug), slug);
        Self {
            project_root,
            store_home,
            store_root: store.root,
            plugins: Vec::new(),
            etiquettes: Vec::new(),
        }
    }

    pub fn with_store_home(mut self, store_home: impl Into<PathBuf>) -> Self {
        self.store_home = store_home.into();
        self
    }

    pub fn with_store_root(mut self, store_root: impl Into<PathBuf>) -> Self {
        let store_root = store_root.into();
        self.store_home = store_root.clone();
        self.store_root = store_root;
        self
    }

    pub fn register(mut self, etiquette: &'static dyn Etiquette) -> Self {
        self.etiquettes.push(etiquette);
        self
    }

    pub fn register_plugin(mut self, plugin: &'static dyn Plugin) -> Self {
        self.plugins.push(plugin);
        self
    }

    pub fn build(self) -> RuntimeSession {
        RuntimeSession {
            project_root: self.project_root,
            store_home: self.store_home,
            store_root: self.store_root,
            plugins: self.plugins,
            etiquettes: self.etiquettes,
        }
    }
}

/// Default session implementation.
pub struct RuntimeSession {
    project_root: PathBuf,
    store_home: PathBuf,
    store_root: PathBuf,
    plugins: Vec<&'static dyn Plugin>,
    etiquettes: Vec<&'static dyn Etiquette>,
}

impl SessionView for RuntimeSession {
    fn project_root(&self) -> &Path {
        &self.project_root
    }

    fn store_root(&self) -> &Path {
        &self.store_root
    }

    fn store_home(&self) -> &Path {
        &self.store_home
    }
}

impl Session for RuntimeSession {
    fn register(&mut self, etiquette: &'static dyn Etiquette) {
        let id = etiquette.id();
        if !self.etiquettes.iter().any(|existing| existing.id() == id) {
            self.etiquettes.push(etiquette);
        }
    }

    fn register_plugin(&mut self, plugin: &'static dyn Plugin) {
        let id = plugin.id();
        if !self.plugins.iter().any(|existing| existing.id() == id) {
            self.plugins.push(plugin);
        }
    }

    #[instrument(skip(self, filter), fields(project_root = %self.project_root.display()))]
    fn run(&self, filter: &dyn RunFilter) -> CordialResult<Box<dyn RunOutcome>> {
        let store = crate::store::StoreLayout::from_root(
            &self.store_root,
            crate::store::project_slug_from_path(&self.project_root),
        );
        store.ensure_dirs()?;

        let etiquettes = resolved_etiquettes(&self.plugins, &self.etiquettes, filter);
        if etiquettes.is_empty() {
            return Ok(Box::new(ConcreteRunOutcome {
                findings: Vec::new(),
                artifacts: Vec::new(),
            }));
        }

        let targets = crate::targets::discover_run_crate_targets(
            &self.plugins,
            &self.project_root,
            self,
            filter,
        )?;
        if targets.is_empty() {
            return Ok(Box::new(ConcreteRunOutcome {
                findings: Vec::new(),
                artifacts: Vec::new(),
            }));
        }

        let loaders = dedupe_loaders(&etiquettes);
        let enrichers = dedupe_enrichers(&etiquettes, &loaders);
        let probes = dedupe_probes(&etiquettes);
        let assessors = dedupe_assessors(&etiquettes);
        let workspace_assessors = dedupe_workspace_assessors(&etiquettes);
        let reporters = dedupe_reporters(&etiquettes);

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
                self,
                filter,
                &loaders,
                &enrichers,
            )?;
        }

        for target in &targets {
            let mut crate_ir = CrateIr::new(&target.crate_name);

            for loader in &loaders {
                let view = loader.load(self, target)?;
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

            for enricher in &enrichers {
                let load = select_load_view(*enricher, &load_views, &target.crate_name)?;
                let mut view = CrateViewMut {
                    workspace: &mut workspace,
                    crate_name: target.crate_name.clone(),
                };
                enricher.enrich(&mut view, load, self)?;
            }

            let cached = workspace
                .crate_ir(&target.crate_name)
                .ok_or_else(|| CordialError::invariant("crate ir must exist"))?;
            cached.write_cache(&store.ir_cache_path(&target.crate_name))?;
            let digest = crate::cache_digest::IrCacheDigest::compute(
                target,
                &enricher_id_refs,
                &load_views,
            )?;
            digest.write(&crate::cache_digest::IrCacheDigest::cache_path(
                &store.cache_dir(),
                &target.crate_name,
            ))?;

            let crate_view = CrateView {
                workspace: &workspace,
                crate_name: target.crate_name.clone(),
            };
            for probe in &probes {
                let mut found = probe.probe(&crate_view, self)?;
                markers_by_crate
                    .entry(target.crate_name.clone())
                    .or_default()
                    .append(&mut found);
            }
        }

        let etiquette_ids: Vec<&str> = etiquettes.iter().map(|etiquette| etiquette.id()).collect();
        let mut all_findings: Vec<Box<dyn Finding>> = Vec::new();

        for target in &targets {
            let markers = markers_by_crate
                .get(&target.crate_name)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let marker_refs: Vec<&dyn Marker> = markers
                .iter()
                .map(|marker| marker.as_ref() as &dyn Marker)
                .collect();
            let crate_view = CrateView {
                workspace: &workspace,
                crate_name: target.crate_name.clone(),
            };

            let mut crate_findings: Vec<Box<dyn Finding>> = Vec::new();
            for assessor in &assessors {
                let relevant: Vec<&dyn Marker> = marker_refs
                    .iter()
                    .copied()
                    .filter(|marker| assessor.consumes().contains(&marker.label()))
                    .collect();
                let mut findings = assessor.assess(&relevant, &crate_view, self)?;
                crate_findings.append(&mut findings);
            }

            let exception_sets =
                crate::exceptions::load_exception_sets(&store, &etiquette_ids, &target.crate_name)?;
            crate_findings =
                crate::exceptions::apply_exception_sets(crate_findings, &exception_sets);
            all_findings.extend(crate_findings);
        }

        #[cfg(feature = "shadow")]
        {
            crate::shadow::preload_shadow_pair_crates(
                &mut workspace,
                self,
                filter,
                &loaders,
                &enrichers,
            )?;
        }

        for assessor in &workspace_assessors {
            all_findings.extend(assessor.assess(&workspace, self, filter)?);
        }

        let primary_name = targets
            .first()
            .map(|target| target.crate_name.clone())
            .ok_or_else(|| CordialError::invariant("workspace missing crate targets"))?;
        let crate_view = CrateView {
            workspace: &workspace,
            crate_name: primary_name,
        };

        let finding_refs: Vec<&dyn Finding> = all_findings
            .iter()
            .map(|finding| finding.as_ref() as &dyn Finding)
            .collect();

        let mut all_artifacts: Vec<Box<dyn Artifact>> = Vec::new();
        for reporter in &reporters {
            let mut artifacts = reporter.render(&finding_refs, &crate_view, self)?;
            all_artifacts.append(&mut artifacts);
        }

        let rollup = RollupReporter;
        let mut rollup_artifacts = rollup.render(&finding_refs, &crate_view, self)?;
        all_artifacts.append(&mut rollup_artifacts);

        #[cfg(feature = "quality")]
        let includes_quality = run_includes_quality(&self.plugins, filter, &etiquettes);
        #[cfg(not(feature = "quality"))]
        let includes_quality = false;

        #[cfg(feature = "quality")]
        if includes_quality {
            let quality_report = QualityReportReporter;
            let mut quality_artifacts = quality_report.render(&finding_refs, &crate_view, self)?;
            all_artifacts.append(&mut quality_artifacts);
        }

        #[cfg(any(
            feature = "homecoming_std",
            feature = "amenable_std",
            feature = "elicitation"
        ))]
        if run_includes_coverage(&self.plugins, filter, &etiquettes) {
            let summary = build_coverage_summary(
                &self.plugins,
                &etiquette_ids,
                filter,
                self,
                &finding_refs,
                &workspace,
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

        for artifact in &all_artifacts {
            let path = store.findings_dir().join(artifact.name());
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut file = std::fs::File::create(&path)?;
            artifact.write_to(&mut file)?;
        }

        Ok(Box::new(ConcreteRunOutcome {
            findings: all_findings,
            artifacts: all_artifacts,
        }))
    }
}

struct ConcreteRunOutcome {
    findings: Vec<Box<dyn Finding>>,
    artifacts: Vec<Box<dyn Artifact>>,
}

impl RunOutcome for ConcreteRunOutcome {
    fn findings(&self) -> Box<dyn Iterator<Item = &dyn Finding> + '_> {
        Box::new(
            self.findings
                .iter()
                .map(|finding| finding.as_ref() as &dyn Finding),
        )
    }

    fn artifacts(&self) -> Box<dyn Iterator<Item = &dyn Artifact> + '_> {
        Box::new(
            self.artifacts
                .iter()
                .map(|artifact| artifact.as_ref() as &dyn Artifact),
        )
    }
}

/// Run all registered etiquettes.
#[derive(Debug, Default, Clone, Copy)]
pub struct RunAll;

impl RunFilter for RunAll {
    fn plugins(&self) -> Option<&[&str]> {
        None
    }

    fn etiquettes(&self) -> Option<&[&str]> {
        None
    }

    fn crates(&self) -> Option<&[&str]> {
        None
    }
}

fn resolved_etiquettes(
    plugins: &[&'static dyn Plugin],
    direct_etiquettes: &[&'static dyn Etiquette],
    filter: &dyn RunFilter,
) -> Vec<&'static dyn Etiquette> {
    let plugin_slice: Vec<&'static dyn Plugin> = if plugins.is_empty() {
        Vec::new()
    } else {
        selected_plugins(plugins, filter.plugins())
    };

    let mut merged = if plugin_slice.is_empty() {
        direct_etiquettes.to_vec()
    } else {
        etiquettes_from_plugins(&plugin_slice)
    };

    for etiquette in direct_etiquettes {
        if !merged
            .iter()
            .any(|existing| existing.id() == etiquette.id())
        {
            merged.push(*etiquette);
        }
    }

    match filter.etiquettes() {
        Some(ids) => merged
            .into_iter()
            .filter(|etiquette| ids.contains(&etiquette.id()))
            .collect(),
        None => merged,
    }
}

fn dedupe_loaders(etiquettes: &[&'static dyn Etiquette]) -> Vec<&'static dyn Loader> {
    dedupe_hooks(
        etiquettes
            .iter()
            .flat_map(|etiquette| etiquette.loaders().iter().copied()),
    )
}

fn dedupe_enrichers(
    etiquettes: &[&'static dyn Etiquette],
    loaders: &[&'static dyn Loader],
) -> Vec<&'static dyn IrEnricher> {
    static PATH_INDEX: PathIndexEnricher = PathIndexEnricher;
    static SYN_DOC_LINK: SynDocLinkEnricher = SynDocLinkEnricher;
    let mut out = dedupe_hooks(
        etiquettes
            .iter()
            .flat_map(|etiquette| etiquette.enrichers().iter().copied()),
    );
    let dual_inventory = loaders_include_source_and_rustdoc(loaders);
    #[cfg(feature = "rustdoc")]
    let has_rustdoc = loaders
        .iter()
        .any(|loader| loader.id() == RustdocLoader::ID);
    if !out.is_empty() || dual_inventory {
        #[cfg(feature = "rustdoc")]
        if has_rustdoc
            && !out
                .iter()
                .any(|enricher| enricher.id() == RustdocStructureEnricher::ID)
        {
            static RUSTDOC_STRUCTURE: RustdocStructureEnricher = RustdocStructureEnricher;
            out.insert(0, &RUSTDOC_STRUCTURE);
        }
        if !out
            .iter()
            .any(|enricher| enricher.id() == PathIndexEnricher::ID)
        {
            out.push(&PATH_INDEX);
        }
        if dual_inventory
            && !out
                .iter()
                .any(|enricher| enricher.id() == SynDocLinkEnricher::ID)
        {
            out.push(&SYN_DOC_LINK);
        }
    }
    out.sort_by_key(|enricher| enricher.priority());
    out
}

fn loaders_include_source_and_rustdoc(loaders: &[&'static dyn Loader]) -> bool {
    let mut has_source = false;
    #[cfg_attr(not(feature = "rustdoc"), allow(unused_mut))]
    let mut has_rustdoc = false;
    for loader in loaders {
        if loader.id() == SourceLoader::ID {
            has_source = true;
        }
        #[cfg(feature = "rustdoc")]
        if loader.id() == RustdocLoader::ID {
            has_rustdoc = true;
        }
    }
    #[cfg(feature = "rustdoc")]
    {
        has_source && has_rustdoc
    }
    #[cfg(not(feature = "rustdoc"))]
    {
        false
    }
}

fn select_load_view<'a>(
    enricher: &dyn IrEnricher,
    load_views: &'a HashMap<String, Box<dyn LoadView>>,
    crate_name: &str,
) -> CordialResult<&'a dyn LoadView> {
    let preferred = enricher.required_loader();
    if let Some(view) = load_views.get(&format!("{crate_name}:{preferred}")) {
        return Ok(view.as_ref());
    }
    if let Some(view) = load_views.get(&format!("{crate_name}:{}", SourceLoader::ID)) {
        return Ok(view.as_ref());
    }
    #[cfg(feature = "rustdoc")]
    if let Some(view) = load_views.get(&format!("{crate_name}:{}", RustdocLoader::ID)) {
        return Ok(view.as_ref());
    }
    Err(CordialError::invariant(format!(
        "no load view available for enricher `{}` on crate `{crate_name}`",
        enricher.id()
    )))
}

fn dedupe_probes(etiquettes: &[&'static dyn Etiquette]) -> Vec<&'static dyn Probe> {
    dedupe_hooks(
        etiquettes
            .iter()
            .flat_map(|etiquette| etiquette.probes().iter().copied()),
    )
}

fn dedupe_assessors(etiquettes: &[&'static dyn Etiquette]) -> Vec<&'static dyn Assessor> {
    dedupe_hooks(
        etiquettes
            .iter()
            .flat_map(|etiquette| etiquette.assessors().iter().copied()),
    )
}

fn dedupe_workspace_assessors(
    etiquettes: &[&'static dyn Etiquette],
) -> Vec<&'static dyn WorkspaceAssessor> {
    dedupe_hooks(
        etiquettes
            .iter()
            .flat_map(|etiquette| etiquette.workspace_assessors().iter().copied()),
    )
}

fn dedupe_reporters(etiquettes: &[&'static dyn Etiquette]) -> Vec<&'static dyn Reporter> {
    dedupe_hooks(
        etiquettes
            .iter()
            .flat_map(|etiquette| etiquette.reporters().iter().copied()),
    )
}

fn dedupe_hooks<'a, T: Hook + ?Sized>(items: impl Iterator<Item = &'a T>) -> Vec<&'a T> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in items {
        if seen.insert(item.hook_id()) {
            out.push(item);
        }
    }
    out
}

trait Hook {
    fn hook_id(&self) -> &str;
}

impl Hook for dyn Loader {
    fn hook_id(&self) -> &str {
        self.id()
    }
}

impl Hook for dyn IrEnricher {
    fn hook_id(&self) -> &str {
        self.id()
    }
}

impl Hook for dyn Probe {
    fn hook_id(&self) -> &str {
        self.id()
    }
}

impl Hook for dyn Assessor {
    fn hook_id(&self) -> &str {
        self.id()
    }
}

impl Hook for dyn WorkspaceAssessor {
    fn hook_id(&self) -> &str {
        self.id()
    }
}

impl Hook for dyn Reporter {
    fn hook_id(&self) -> &str {
        self.id()
    }
}
