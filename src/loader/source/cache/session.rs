//! One load-and-probe run: the token its views share and the guard that scopes it.

use crate::loader::{Reporter, SemanticCargoOptions, SourceDatabase};
use crate::run_cache::RunCache;
use crate::{CfgPlanning, CordialResult, CrateIr};
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use tracing::instrument;
thread_local! {
    pub(super) static SESSION_TOKEN: RefCell<Option<Arc<CacheToken>>> = const { RefCell::new(None) };
}

/// How to load the real database when a run turns out to need it.
#[derive(Debug, Clone)]
struct LoadRecipe {
    root: PathBuf,
    options: SemanticCargoOptions,
}

/// The run's database: empty until something needs text, loaded only when
/// something needs the analysis itself.
#[derive(Debug, Default)]
struct Slot {
    database: Option<SourceDatabase>,
    loaded: bool,
}

/// Identity of one load-and-probe run, shared by every view created inside it.
/// It owns the run's single source database, created on first use because most
/// views never need one, and the cached results that let a run skip loading it.
pub(in crate::loader::source) struct CacheToken {
    id: u64,
    slot: Mutex<Slot>,
    recipe: Mutex<Option<LoadRecipe>>,
    cache: Mutex<Option<Arc<RunCache>>>,
    reporter: Mutex<Reporter>,
}

impl std::fmt::Debug for CacheToken {
    #[tracing::instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("CacheToken").field(&self.id).finish()
    }
}

impl Default for CacheToken {
    #[tracing::instrument(level = "debug")]
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            slot: Mutex::new(Slot::default()),
            recipe: Mutex::new(None),
            cache: Mutex::new(None),
            reporter: Mutex::new(Reporter::default()),
        }
    }
}

#[instrument(level = "debug", skip(mutex))]
fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl CacheToken {
    /// Give this run the database that already holds its Cargo workspace.
    #[tracing::instrument(level = "trace", skip(self, database))]
    fn install(&self, database: SourceDatabase) {
        *locked(&self.slot) = Slot {
            database: Some(database),
            loaded: true,
        };
    }

    /// Run `f` against this run's database, creating a context-less one if the
    /// run has none yet. Enough for reading and parsing text; analysis needs
    /// [`Self::with_loaded_database`]. The lock is not reentrant: `f` must not
    /// request the database again.
    #[tracing::instrument(level = "trace", skip(self, f))]
    pub(super) fn with_database<R>(&self, f: impl FnOnce(&mut SourceDatabase) -> R) -> R {
        let mut slot = locked(&self.slot);
        f(slot.database.get_or_insert_with(SourceDatabase::new))
    }

    /// Run `f` against the database that holds the loaded Cargo workspace,
    /// loading it now if this run has deferred that until something needs it.
    #[tracing::instrument(level = "trace", skip(self, f), err(level = "warn"))]
    fn with_loaded_database<R>(
        &self,
        f: impl FnOnce(&mut SourceDatabase) -> CordialResult<R>,
    ) -> CordialResult<R> {
        let mut slot = locked(&self.slot);
        if !slot.loaded
            && let Some(recipe) = locked(&self.recipe).clone()
        {
            let reporter = locked(&self.reporter).clone();
            let database =
                SourceDatabase::load_reporting(&recipe.root, &recipe.options, &reporter)?;
            *slot = Slot {
                database: Some(database),
                loaded: true,
            };
        }
        f(slot.database.get_or_insert_with(SourceDatabase::new))
    }

    /// Say future long phases through `reporter`.
    #[tracing::instrument(level = "trace", skip(self, reporter))]
    fn set_reporter(&self, reporter: Reporter) {
        if let Some(database) = locked(&self.slot).database.as_mut() {
            database.set_reporter(reporter.clone());
        }
        *locked(&self.reporter) = reporter;
    }

    /// A target's IR: from the cached results when they hold it, otherwise
    /// projected from the database, which is then recorded for the next run.
    #[tracing::instrument(level = "debug", skip(self, target), err(level = "warn"))]
    #[hotpath::measure(impl_type = "CacheToken")]
    pub(super) fn project_target(
        &self,
        target: &crate::loader::CrateTarget,
    ) -> CordialResult<CrateIr> {
        let cache = locked(&self.cache).clone();
        let name = target.crate_name();
        if let Some(cache) = &cache {
            match cache.read_crate_ir(name) {
                Ok(Some(ir)) => {
                    tracing::info!(target: "cordial::cache", crate_name = %name, "result cache hit");
                    locked(&self.reporter).say(format!("Reusing the cached analysis of {name}"));
                    return Ok(ir);
                }
                Ok(None) => self.explain_miss(cache, name),
                Err(error) => {
                    tracing::warn!(target: "cordial::cache", error = ?error, "cached analysis unreadable; analysing again");
                }
            }
        }
        let ir = self.with_loaded_database(|database| database.project_target(target))?;
        if let Some(cache) = &cache
            && let Err(error) = cache.write_crate_ir(name, &ir)
        {
            tracing::warn!(target: "cordial::cache", error = ?error, "could not record the analysis");
        }
        Ok(ir)
    }

    /// The workspace path-inclusion facts: cached when this run's results hold
    /// them, otherwise projected from the database and recorded.
    #[cfg(feature = "derives")]
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "CacheToken")]
    pub(super) fn path_inclusions(&self) -> CordialResult<crate::ir::PathInclusionFacts> {
        let cache = locked(&self.cache).clone();
        if let Some(cache) = &cache {
            match cache.read_path_inclusions() {
                Ok(Some(facts)) => return Ok(facts),
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(target: "cordial::cache", error = ?error, "cached path inclusions unreadable; analysing again");
                }
            }
        }
        let facts = self.with_loaded_database(|database| database.project_path_inclusions())?;
        if let Some(cache) = &cache
            && let Err(error) = cache.write_path_inclusions(&facts)
        {
            tracing::warn!(target: "cordial::cache", error = ?error, "could not record the path inclusions");
        }
        Ok(facts)
    }

    /// Say why the cached results do not apply, naming the inputs that changed.
    #[tracing::instrument(level = "debug", skip(self, cache))]
    fn explain_miss(&self, cache: &RunCache, crate_name: &str) {
        let changed = cache.changed_since_last();
        let shown: Vec<_> = changed.iter().take(3).cloned().collect();
        tracing::info!(
            target: "cordial::cache",
            crate_name,
            changed = changed.len(),
            first = ?shown,
            "result cache miss"
        );
        let reason = if changed.is_empty() {
            "no earlier analysis".to_string()
        } else {
            format!(
                "{} input(s) changed, e.g. {}",
                changed.len(),
                shown.join(", ")
            )
        };
        locked(&self.reporter).say(format!("Analysing {crate_name} ({reason})"));
    }
}

/// Retain parsed files across all crate views in one load-and-probe run.
pub(crate) struct ParsedSourceSession {
    previous: Option<Arc<CacheToken>>,
    token: Arc<CacheToken>,
}

impl ParsedSourceSession {
    #[tracing::instrument(level = "debug")]
    pub(crate) fn begin() -> Self {
        let token = Arc::new(CacheToken::default());
        let previous = SESSION_TOKEN.with(|current| current.replace(Some(token.clone())));
        Self { previous, token }
    }

    /// Begin a run whose database holds the Cargo workspace at `root`, loaded
    /// now. A root that is not a loadable crate or workspace is an error.
    #[tracing::instrument(level = "debug", err(level = "warn"), skip(reporter, planning))]
    pub(crate) fn begin_in(
        root: &std::path::Path,
        reporter: &Reporter,
        planning: CfgPlanning,
    ) -> CordialResult<Self> {
        let database = SourceDatabase::load_reporting(
            root,
            &SemanticCargoOptions::default().with_cfg_planning(planning),
            reporter,
        )?;
        let session = Self::begin();
        session.token.install(database);
        Ok(session)
    }

    /// Begin a run that loads the workspace only if its results are not already
    /// cached. `cache` is `None` when the inputs could not be gathered, in which
    /// case the workspace loads on first use and nothing is reused.
    #[tracing::instrument(level = "debug", skip(cache, planning))]
    pub(crate) fn begin_deferred(
        root: &std::path::Path,
        planning: CfgPlanning,
        cache: Option<RunCache>,
    ) -> Self {
        let session = Self::begin();
        *locked(&session.token.recipe) = Some(LoadRecipe {
            root: root.to_path_buf(),
            options: SemanticCargoOptions::default().with_cfg_planning(planning),
        });
        *locked(&session.token.cache) = cache.map(Arc::new);
        session
    }

    /// Say the database's long phases and cache decisions through `reporter`.
    #[tracing::instrument(level = "trace", skip(self, reporter))]
    pub(crate) fn set_reporter(&self, reporter: Reporter) {
        self.token.set_reporter(reporter);
    }

    /// Project one target's source and semantic evidence: from the cached
    /// results when they hold it, otherwise from the run database.
    #[tracing::instrument(level = "debug", skip(self, target), err(level = "warn"))]
    pub(crate) fn project_target(
        &self,
        target: &crate::loader::CrateTarget,
    ) -> CordialResult<CrateIr> {
        self.token.project_target(target)
    }
}

impl Drop for ParsedSourceSession {
    #[tracing::instrument(level = "trace", skip(self))]
    fn drop(&mut self) {
        SESSION_TOKEN.with(|current| {
            current.replace(self.previous.take());
        });
    }
}
