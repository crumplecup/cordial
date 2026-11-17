//! Run the collectors that read only a file's tree on every file, a chunk of files at a time,
//! before the steps that merge their results.
//!
//! Each step asks, for each file in turn, for what one collector found in it. Those are
//! projections over one extraction per file (`scan_queries::extract_file`), which parses the file
//! and runs all the collectors on it. Computing the extractions here, file by file and in chunks
//! with the syntax trees dropped between them, makes the steps find results ready: a file is parsed
//! once, whatever the capacity of the cache that holds trees, and a restored run, whose results are
//! valid, has nothing to compute here.

use std::path::{Path, PathBuf};

use ra_ap_vfs::FileId;
use tracing::instrument;

use super::{SourceDatabase, scan_queries};
use crate::CordialResult;

/// The Rust files under the trees a collector scans, in the order the collectors walk them.
#[instrument(level = "debug", err(level = "warn"))]
#[hotpath::measure]
fn scanned_files(root: &Path) -> CordialResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    for tree in crate::loader::quality_scan_trees(root) {
        for entry in walkdir::WalkDir::new(&tree).sort_by_file_name() {
            let entry = entry.map_err(anyhow::Error::new)?;
            if entry.file_type().is_file()
                && entry.path().extension().is_some_and(|ext| ext == "rs")
                && !crate::loader::path_has_fixtures(entry.path(), root)
            {
                files.push(entry.path().to_path_buf());
            }
        }
    }
    Ok(files)
}

impl SourceDatabase {
    /// Compute, for each file the collectors scan under `root`, what they find in it.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn prime_file_scans(&mut self, root: &Path) -> CordialResult<()> {
        if self
            .cargo_options
            .as_ref()
            .is_some_and(|options| *options.skip_file_scan_priming())
        {
            return Ok(());
        }
        let files: Vec<(FileId, PathBuf)> = scanned_files(root)?
            .into_iter()
            .filter_map(|path| self.file_id(&path).ok().map(|file| (file, path)))
            .collect();
        let root = root.to_path_buf();
        self.run_in_chunks("file scans", &files, &|db, (file, path)| {
            scan_queries::prime_extraction(db, *file, path, &root);
        });
        Ok(())
    }
}
