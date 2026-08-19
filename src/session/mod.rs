use std::path::{Path, PathBuf};

use crate::error::CordialResult;
use crate::etiquette::Etiquette;
use crate::objects::{Artifact, Finding};
use crate::plugin::Plugin;

use tracing::instrument;
mod resolve;
mod run;

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

    #[instrument(level = "trace", skip(self, store_home))]
    pub fn with_store_home(mut self, store_home: impl Into<PathBuf>) -> Self {
        self.store_home = store_home.into();
        self
    }

    #[instrument(level = "trace", skip(self, store_root))]
    pub fn with_store_root(mut self, store_root: impl Into<PathBuf>) -> Self {
        let store_root = store_root.into();
        self.store_home = store_root.clone();
        self.store_root = store_root;
        self
    }

    #[instrument(level = "trace", skip(self, etiquette))]
    pub fn register(mut self, etiquette: &'static dyn Etiquette) -> Self {
        self.etiquettes.push(etiquette);
        self
    }

    #[instrument(level = "trace", skip(self, plugin))]
    pub fn register_plugin(mut self, plugin: &'static dyn Plugin) -> Self {
        self.plugins.push(plugin);
        self
    }

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

/// Default session implementation.
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

/// Run all registered etiquettes.
#[derive(Debug, Default, Clone, Copy)]
pub struct RunAll;

impl RunFilter for RunAll {
    #[instrument(level = "trace", skip(self))]
    fn plugins(&self) -> Option<&[&str]> {
        None
    }

    #[instrument(level = "trace", skip(self))]
    fn etiquettes(&self) -> Option<&[&str]> {
        None
    }

    #[instrument(level = "trace", skip(self))]
    fn crates(&self) -> Option<&[&str]> {
        None
    }
}
