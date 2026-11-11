//! One load-and-probe run: the token its views share and the guard that scopes it.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

thread_local! {
    pub(super) static SESSION_TOKEN: RefCell<Option<Arc<CacheToken>>> = const { RefCell::new(None) };
}

/// Identity of one load-and-probe run, shared by every view created inside it.
/// It owns the run's single source database, created on first use because most
/// views never need one.
pub(in crate::loader::source) struct CacheToken(u64, Mutex<Option<crate::loader::SourceDatabase>>);

impl std::fmt::Debug for CacheToken {
    #[tracing::instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("CacheToken").field(&self.0).finish()
    }
}

impl Default for CacheToken {
    #[tracing::instrument(level = "debug")]
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed), Mutex::new(None))
    }
}

impl CacheToken {
    /// Give this run the database that already holds its Cargo workspace.
    #[tracing::instrument(level = "trace", skip(self, database))]
    fn install(&self, database: crate::loader::SourceDatabase) {
        *self.1.lock().unwrap_or_else(PoisonError::into_inner) = Some(database);
    }

    /// Run `f` against this run's database, creating a context-less one if the
    /// run has none yet. The lock is not reentrant: `f` must not request the
    /// database again.
    #[tracing::instrument(level = "trace", skip(self, f))]
    pub(super) fn with_database<R>(
        &self,
        f: impl FnOnce(&mut crate::loader::SourceDatabase) -> R,
    ) -> R {
        let mut slot = self.1.lock().unwrap_or_else(PoisonError::into_inner);
        f(slot.get_or_insert_with(crate::loader::SourceDatabase::new))
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

    /// Begin a run whose database holds the Cargo workspace at `root`.
    /// A root that is not a loadable crate or workspace is an error.
    #[tracing::instrument(level = "debug", err(level = "warn"), skip(reporter, planning))]
    pub(crate) fn begin_in(
        root: &std::path::Path,
        reporter: &crate::loader::Reporter,
        planning: crate::CfgPlanning,
    ) -> crate::CordialResult<Self> {
        let database = crate::loader::SourceDatabase::load_reporting(
            root,
            &crate::loader::SemanticCargoOptions::default().with_cfg_planning(planning),
            reporter,
        )?;
        let session = Self::begin();
        session.token.install(database);
        Ok(session)
    }

    /// Say the database's long projection phases through `reporter` from now on.
    #[tracing::instrument(level = "trace", skip(self, reporter))]
    pub(crate) fn set_reporter(&self, reporter: crate::loader::Reporter) {
        self.token
            .with_database(|database| database.set_reporter(reporter));
    }

    /// Project one target's source and semantic evidence from the run database.
    #[tracing::instrument(level = "debug", skip(self, target), err(level = "warn"))]
    pub(crate) fn project_target(
        &self,
        target: &crate::loader::CrateTarget,
    ) -> crate::CordialResult<crate::ir::CrateIr> {
        self.token
            .with_database(|database| database.project_target(target))
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
