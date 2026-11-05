//! Snapshot-bound Verus dialect evidence owned by the shared graph.
use crate::loader::{
    SourceLoadView, module_path_from_src_file, path_has_fixtures, quality_scan_trees,
};
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
    #[tracing::instrument(level = "debug", skip(source), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceVerus")]
    pub(crate) fn collect(source: &SourceLoadView, root: &Path) -> CordialResult<Self> {
        Self::collect_with(root, |path, tree| {
            let file = source.parsed_file(path, "source-verus")?;
            Ok(VerusFile::new(
                FileSnapshot::new(path.to_path_buf(), file.source_digest()),
                crate::verus_ir::shared_file_facts(
                    &file,
                    &module_path_from_src_file(tree, path).join("::"),
                )?,
            ))
        })
    }

    #[tracing::instrument(level = "debug", skip(collect), err(level = "warn"))]
    pub(crate) fn collect_with(
        root: &Path,
        mut collect: impl FnMut(&Path, &Path) -> CordialResult<VerusFile>,
    ) -> CordialResult<Self> {
        let mut files = Vec::new();
        for tree in quality_scan_trees(root) {
            for entry in walkdir::WalkDir::new(&tree).sort_by_file_name() {
                let entry = entry.map_err(anyhow::Error::new)?;
                if entry.file_type().is_file()
                    && entry.path().extension().is_some_and(|ext| ext == "rs")
                    && !path_has_fixtures(entry.path(), root)
                {
                    files.push(collect(entry.path(), &tree)?);
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
