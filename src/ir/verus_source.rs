//! Snapshot-bound Verus dialect evidence owned by the shared graph.
use crate::loader::path_has_fixtures;
use crate::{CordialResult, FileSnapshot, VerusCrateIr};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Required Verus inventory on the crate graph root.
pub const ATTR_SOURCE_VERUS: &str = "source_verus";

/// Dialect facts for every scanned file, including files without Verus blocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceVerus {
    crate_root: PathBuf,
    files: Vec<VerusFile>,
}

/// Facts tied to the complete outer Rust source snapshot.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
pub struct VerusFile {
    snapshot: FileSnapshot,
    facts: VerusCrateIr,
}

impl SourceVerus {
    #[tracing::instrument(level = "debug", skip(collect), err(level = "warn"))]
    #[hotpath::measure]
    pub(crate) fn collect_with(
        root: &Path,
        held: &crate::loader::held_files::HeldFiles,
        mut collect: impl FnMut(&Path, &Path) -> CordialResult<VerusFile>,
    ) -> CordialResult<Self> {
        let mut files = Vec::new();
        for tree in held.scan_trees(root) {
            for path in held.under(&tree) {
                if !path_has_fixtures(path, root) {
                    files.push(collect(path, &tree)?);
                }
            }
        }
        files.sort_by(|a, b| a.snapshot.path().cmp(b.snapshot.path()));
        Ok(Self {
            crate_root: root.to_path_buf(),
            files,
        })
    }

    /// Merge persisted facts without a workspace cache, source reads, or parsing.
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn facts(&self) -> VerusCrateIr {
        VerusCrateIr::new(
            self.files
                .iter()
                .flat_map(|file| file.facts.functions().clone())
                .collect(),
            self.files
                .iter()
                .flat_map(|file| file.facts.enums().clone())
                .collect(),
        )
    }
}
