//! Canonical source inputs, tracked queries, and shared IR projection.
use tracing::instrument;
#[cfg(feature = "antipatterns")]
mod antipattern_queries;
#[cfg(feature = "internal_error_chain")]
mod apply_queries;
#[cfg(feature = "internal_error_chain")]
mod architecture_queries;
mod audit;
mod cold_texts;
mod context_crates;
mod context_shares;
mod contexts;
#[cfg(any(
    feature = "antipatterns",
    feature = "panics",
    feature = "internal_error_chain",
    feature = "derives",
    feature = "cli_layout"
))]
mod crate_files;
mod def_map_cutoff;
mod edition;
mod eviction;
mod external;
#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir",
    feature = "visibility",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "cli_layout"
))]
mod file_scans;
mod footprint;
mod identity;
#[cfg(feature = "tracing")]
mod islands;
mod item_key;
mod items;
#[cfg(feature = "cli_layout")]
mod layout_queries;
#[cfg(any(
    feature = "_quality_support",
    feature = "verus_ir",
    feature = "cli_layout"
))]
mod listing;
#[cfg(any(feature = "internal_error_chain", feature = "cli_layout"))]
mod module_queries;
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
mod saved;
mod scan_queries;
mod session;
#[cfg(feature = "internal_error_chain")]
mod signature_queries;
mod spill;
mod state_chunks;
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
pub use saved::SavedState;
pub use session::SyncOutcome;
pub use state_chunks::{
    CHUNKS, ChunkDir, ChunkEntry, ChunkError, ChunkReadError, Chunked, ChunkedReader,
    chunked_reader, write_chunked,
};
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
    // The copies contexts made, which a later context takes when it differs from the one that made
    // them in nothing the crate names.
    shared_copies: std::collections::HashMap<context_shares::CopyKey, context_shares::SharedCopy>,
    // Dependency packages of the loaded Cargo graph, kept out of the context
    // identity so snapshots do not carry the whole dependency tree.
    external_packages: Vec<crate::SemanticPackageContext>,
    cargo_options: Option<crate::SemanticCargoOptions>,
    // Keep the expansion service alive for this entire analysis session.
    proc_macro_server: Option<ra_ap_proc_macro_api::ProcMacroClient>,
    // Progress messages for long loads and projections.
    reporter: Reporter,
    // Where this session's database is saved for the next run, and whether it has been.
    saved: Option<SavedState>,
    saved_once: bool,
    // How many persisted queries the process had executed when this session was made, so that
    // a restored session can tell whether it has computed anything a saved state lacks.
    persisted_at_load: u64,
    // The file spilled values are in, once a run has spilled any.
    spill: Option<spill::Spilling>,
    // Whether the database was restored from a saved state.
    restored: bool,
    // Where the pass over the cfg contexts is taken up from, when an earlier process of this run
    // did some of it.
    resume_contexts: Option<contexts::ContextsState>,
    #[cfg(feature = "cfg_hygiene")]
    workspace_root: Option<std::path::PathBuf>,
}
