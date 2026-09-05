//! Runtime orchestration for registered plugins and etiquettes.
//!
//! A [`Session`] owns the project paths and the set of available extensions.
//! A [`RunFilter`] narrows one invocation to selected plugins, etiquettes, or
//! crate targets. The runtime resolves those choices into a hook pipeline,
//! deduplicates hooks by id, builds crate IR, runs probes and assessors, applies
//! exception sets, and writes reporter artifacts into the local store.
//!
//! Most callers use [`SessionBuilder`] plus [`RunAll`]. Custom frontends should
//! implement [`RunFilter`] when they need CLI-like selection without changing
//! which plugins or etiquettes are registered on the session.

use std::path::{Path, PathBuf};

use crate::error::CordialResult;
use crate::etiquette::Etiquette;
use crate::objects::{Artifact, Finding};
use crate::plugin::Plugin;

use tracing::instrument;
mod resolve;
mod run;

/// Read-only paths and run context shared with hooks.
///
/// Hook implementations receive a session view instead of owning path
/// discovery. Treat these paths as stable for the duration of one run.
pub trait SessionView: Send + Sync {
    /// Root of the project being analyzed.
    ///
    /// This is the directory passed to [`SessionBuilder::new`] or selected by
    /// the CLI. Workspace discovery, source loading, and config lookup are
    /// anchored here.
    fn project_root(&self) -> &Path;
    /// Project-specific store root for generated cache and report files.
    ///
    /// Reporters write artifacts below this tree, while loaders and enrichers
    /// may use it for replayable cache inputs.
    fn store_root(&self) -> &Path;
    /// Shared cordial store home.
    ///
    /// Defaults to `~/.cordial`, unless the caller supplies an override through
    /// the CLI, environment, or builder. Global `cordial.toml` config is read
    /// from this directory.
    fn store_home(&self) -> &Path;
}

/// Per-run selection over registered plugins, resolved etiquettes, and crates.
///
/// Filters never register new work. They only choose from the plugins and
/// etiquettes already present on a [`Session`]. Returning [`None`] means "do
/// not filter this dimension"; returning an empty slice means "select nothing"
/// for that dimension.
pub trait RunFilter: Send + Sync {
    /// Plugin ids to activate for this run.
    ///
    /// The ids are compared with [`Plugin::id`]. When no plugin filter is
    /// supplied, all registered plugins are candidates.
    fn plugins(&self) -> Option<&[String]> {
        None
    }

    /// Etiquette ids to activate after plugin expansion.
    ///
    /// The ids are compared with [`Etiquette::id`]. Directly registered
    /// etiquettes are merged with plugin-provided etiquettes before this filter
    /// is applied.
    fn etiquettes(&self) -> Option<&[String]>;
    /// Workspace crate names to analyze.
    ///
    /// This is a multi-crate selector. If [`Self::crate_name`] returns
    /// [`Some`], the singular crate name takes precedence.
    fn crates(&self) -> Option<&[&str]> {
        None
    }
    /// Singular workspace crate name to analyze.
    ///
    /// CLI callers use this for `--crate`; it overrides [`Self::crates`] in
    /// target discovery.
    fn crate_name(&self) -> Option<&str> {
        None
    }
}

/// Findings and artifacts produced by one completed session run.
///
/// The outcome owns the concrete values while exposing trait-object iterators
/// so callers can count, inspect, or copy results without depending on built-in
/// storage types.
pub trait RunOutcome: Send + Sync {
    /// Iterate over all findings after exception suppression has been applied.
    fn findings(&self) -> Box<dyn Iterator<Item = &dyn Finding> + '_>;
    /// Iterate over all artifacts rendered and written by reporters.
    fn artifacts(&self) -> Box<dyn Iterator<Item = &dyn Artifact> + '_>;
}

/// Orchestrates plugin and etiquette execution.
///
/// A session is the long-lived registry for available plugins and direct
/// etiquettes. The runtime flattens selected plugins into etiquettes, keeps
/// directly registered etiquettes available, deduplicates hook ids, and runs
/// the load → enrich → probe → assess → report pipeline.
pub trait Session: Send + Sync {
    /// Register one directly available etiquette by id.
    ///
    /// Runtime sessions ignore duplicates with the same [`Etiquette::id`].
    fn register(&mut self, etiquette: &'static dyn Etiquette);
    /// Register one directly available plugin by id.
    ///
    /// Runtime sessions ignore duplicates with the same [`Plugin::id`].
    fn register_plugin(&mut self, plugin: &'static dyn Plugin);
    /// Run the selected pipeline and return its findings and artifacts.
    ///
    /// This method creates store directories, applies project config gates,
    /// discovers crate targets, caches IR, applies exception sets, and writes
    /// rendered artifacts under [`SessionView::store_root`].
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

impl std::fmt::Debug for SessionBuilder {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionBuilder")
            .field("project_root", &self.project_root)
            .field("store_home", &self.store_home)
            .field("store_root", &self.store_root)
            .field("plugins", &self.plugins.len())
            .field("etiquettes", &self.etiquettes.len())
            .finish()
    }
}

impl SessionBuilder {
    /// Create a session builder rooted at one project directory.
    ///
    /// The builder derives the default project store from the project path and
    /// the default cordial home.
    #[instrument(level = "debug", skip(project_root), ret)]
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

    /// Return a copy with the shared cordial store home set.
    ///
    /// This controls where global config such as `cordial.toml` is loaded from.
    #[instrument(level = "trace", skip(self, store_home))]
    pub fn with_store_home(mut self, store_home: impl Into<PathBuf>) -> Self {
        self.store_home = store_home.into();
        self
    }

    /// Return a copy with the project-specific store root set.
    ///
    /// This controls where generated caches and report artifacts are written.
    #[instrument(level = "trace", skip(self, store_root))]
    pub fn with_store_root(mut self, store_root: impl Into<PathBuf>) -> Self {
        let store_root = store_root.into();
        self.store_home = store_root.clone();
        self.store_root = store_root;
        self
    }

    /// Add one directly available etiquette before building the runtime session.
    #[instrument(level = "trace", skip(self, etiquette))]
    pub fn register(mut self, etiquette: &'static dyn Etiquette) -> Self {
        self.etiquettes.push(etiquette);
        self
    }

    /// Add one directly available plugin before building the runtime session.
    #[instrument(level = "trace", skip(self, plugin))]
    pub fn register_plugin(mut self, plugin: &'static dyn Plugin) -> Self {
        self.plugins.push(plugin);
        self
    }

    /// Finish the builder and return a runtime session.
    #[instrument(level = "debug", skip(self))]
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

/// Default in-process [`Session`] implementation.
///
/// This type is what the CLI and most library callers use. It has no global
/// mutable state; all registered plugins and etiquettes are explicit fields.
pub struct RuntimeSession {
    pub(super) project_root: PathBuf,
    pub(super) store_home: PathBuf,
    pub(super) store_root: PathBuf,
    pub(super) plugins: Vec<&'static dyn Plugin>,
    pub(super) etiquettes: Vec<&'static dyn Etiquette>,
}

impl SessionView for RuntimeSession {
    #[instrument(level = "trace", skip(self))]
    fn project_root(&self) -> &Path {
        &self.project_root
    }

    #[instrument(level = "trace", skip(self))]
    fn store_root(&self) -> &Path {
        &self.store_root
    }

    #[instrument(level = "trace", skip(self))]
    fn store_home(&self) -> &Path {
        &self.store_home
    }
}

impl Session for RuntimeSession {
    #[instrument(level = "trace", skip(self, etiquette))]
    fn register(&mut self, etiquette: &'static dyn Etiquette) {
        let id = etiquette.id();
        if !self.etiquettes.iter().any(|existing| existing.id() == id) {
            self.etiquettes.push(etiquette);
        }
    }

    #[instrument(level = "trace", skip(self, plugin))]
    fn register_plugin(&mut self, plugin: &'static dyn Plugin) {
        let id = plugin.id();
        if !self.plugins.iter().any(|existing| existing.id() == id) {
            self.plugins.push(plugin);
        }
    }

    #[instrument(level = "trace", skip(self, filter))]
    fn run(&self, filter: &dyn RunFilter) -> CordialResult<Box<dyn RunOutcome>> {
        run::run_session(self, filter)
    }
}

/// Run every registered plugin, etiquette, and workspace crate.
///
/// This is the default filter for whole-project quality and coverage runs.
#[derive(Debug, Default, Clone, Copy)]
pub struct RunAll;

impl RunFilter for RunAll {
    #[instrument(level = "trace", skip(self))]
    fn plugins(&self) -> Option<&[String]> {
        None
    }

    #[instrument(level = "trace", skip(self))]
    fn etiquettes(&self) -> Option<&[String]> {
        None
    }

    #[instrument(level = "trace", skip(self))]
    fn crates(&self) -> Option<&[&str]> {
        None
    }
}
