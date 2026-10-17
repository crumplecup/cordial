//! Informational age and source-change hints for a persisted list.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::instrument;
use walkdir::WalkDir;

use super::Worklist;

/// Approximate changes since a list was built; never invalidates the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_getters::Getters)]
pub struct SoftStaleness {
    age_secs: u64,
    changed_files: usize,
}

impl SoftStaleness {
    /// Whether at least one source or manifest has a newer modification time.
    #[instrument(level = "trace", skip(self))]
    pub fn is_soft_stale(&self) -> bool {
        self.changed_files > 0
    }
}

impl Worklist {
    /// Count Rust sources and Cargo manifests modified after this list was built.
    #[instrument(level = "debug", skip(self, project_root))]
    pub fn staleness(&self, project_root: &Path) -> SoftStaleness {
        let built_at =
            UNIX_EPOCH + std::time::Duration::from_secs(*self.record().built_at_unix_secs());
        let age_secs = SystemTime::now()
            .duration_since(built_at)
            .unwrap_or_default()
            .as_secs();
        let changed_files = WalkDir::new(project_root)
            .into_iter()
            .filter_entry(|entry| {
                let name = entry.file_name().to_string_lossy();
                entry.depth() == 0 || (name != "target" && !name.starts_with('.'))
            })
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.file_type().is_file()
                    && (entry.path().extension().is_some_and(|ext| ext == "rs")
                        || matches!(
                            entry.file_name().to_str(),
                            Some("Cargo.toml" | "Cargo.lock")
                        ))
            })
            .filter(|entry| {
                entry
                    .metadata()
                    .ok()
                    .and_then(|metadata| metadata.modified().ok())
                    .is_some_and(|modified| modified > built_at)
            })
            .count();
        SoftStaleness {
            age_secs,
            changed_files,
        }
    }
}
