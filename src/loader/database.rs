//! Canonical source inputs, tracked queries, and shared IR projection.
use tracing::instrument;
#[cfg(feature = "antipatterns")]
mod antipattern_queries;
#[cfg(feature = "internal_error_chain")]
mod apply_queries;
#[cfg(feature = "internal_error_chain")]
mod architecture_queries;
mod context_crates;
mod contexts;
#[cfg(any(
    feature = "antipatterns",
    feature = "panics",
    feature = "internal_error_chain",
    feature = "derives"
))]
mod crate_files;
mod external;
mod identity;
#[cfg(feature = "tracing")]
mod islands;
mod items;
mod options;
#[cfg(feature = "panics")]
mod panic_queries;
#[cfg(feature = "derives")]
mod path_inclusion_queries;
mod populate;
mod prime;
mod projection;
mod queries;
mod recording;
mod scan_queries;
mod session;
#[cfg(feature = "internal_error_chain")]
mod signature_queries;
#[cfg(feature = "tracing")]
mod tracing_queries;
mod types;

pub(super) use super::native_syntax::DatabaseSource;
pub use context_crates::ContextCrates;
pub use options::{CfgPlanning, SemanticCargoOptions};
#[cfg(any(
    feature = "tracing",
    feature = "internal_error_chain",
    feature = "verus_ir",
    feature = "panics",
    feature = "antipatterns"
))]
pub(super) use queries::SemanticDatabase;
pub use session::SyncOutcome;
/// Where the loader says what it is doing, for the frontend's progress display.
#[derive(Clone)]
pub struct Reporter(std::sync::Arc<dyn Fn(String) + Send + Sync>);

impl Reporter {
    /// Report through `report`.
    #[instrument(level = "debug", skip(report))]
    #[hotpath::measure]
    pub fn new(report: impl Fn(String) + Send + Sync + 'static) -> Self {
        Self(std::sync::Arc::new(report))
    }

    /// Say `message` to whoever is watching.
    #[instrument(level = "trace", skip(self, message))]
    pub fn say(&self, message: impl Into<String>) {
        (self.0)(message.into());
    }
}

impl Default for Reporter {
    #[instrument(level = "debug")]
    #[hotpath::measure]
    fn default() -> Self {
        Self::new(|_| {})
    }
}

impl std::fmt::Debug for Reporter {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Reporter")
    }
}

/// Canonical source database shared by source collection and semantic queries.
/// Revisions are serial; virtual changes never write to the loaded project.
#[derive(Debug)]
pub struct SourceDatabase {
    db: ra_ap_ide_db::RootDatabase,
    vfs: ra_ap_vfs::Vfs,
    revision: u64,
    workspace: Option<crate::SemanticWorkspaceContext>,
    // Contexts added as crates of their own beside the ordinary ones.
    context_crates: Vec<ContextCrates>,
    // Dependency packages of the loaded Cargo graph, kept out of the context
    // identity so snapshots do not carry the whole dependency tree.
    external_packages: Vec<crate::SemanticPackageContext>,
    cargo_options: Option<crate::SemanticCargoOptions>,
    // Keep the expansion service alive for this entire analysis session.
    proc_macro_server: Option<ra_ap_proc_macro_api::ProcMacroClient>,
    // Progress messages for long loads and projections.
    reporter: Reporter,
    #[cfg(feature = "cfg_hygiene")]
    workspace_root: Option<std::path::PathBuf>,
}
