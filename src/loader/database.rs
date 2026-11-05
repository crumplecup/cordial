//! Canonical source inputs, tracked queries, and shared IR projection.
mod options;
mod populate;
mod projection;
mod queries;
mod recording;
mod session;
mod types;

pub(super) use super::native_syntax::DatabaseSource;
pub use options::SemanticCargoOptions;
#[cfg(any(
    feature = "tracing",
    feature = "internal_error_chain",
    feature = "verus_ir"
))]
pub(super) use queries::SemanticDatabase;
/// Canonical source database shared by source collection and semantic queries.
/// Revisions are serial; virtual changes never write to the loaded project.
#[derive(Debug)]
pub struct SourceDatabase {
    db: ra_ap_ide_db::RootDatabase,
    vfs: ra_ap_vfs::Vfs,
    revision: u64,
    workspace: Option<crate::SemanticWorkspaceContext>,
    cargo_options: Option<crate::SemanticCargoOptions>,
    // Keep the expansion service alive for this entire analysis session.
    proc_macro_server: Option<ra_ap_proc_macro_api::ProcMacroClient>,
    #[cfg(feature = "cfg_hygiene")]
    workspace_root: Option<std::path::PathBuf>,
}
