use std::path::Path;

use crate::error::CordialResult;

use super::source::{SourceFile, SourceLoadView, SourceLoader};

use tracing::instrument;
impl SourceLoadView {
    /// Load the same source view used by the session for a crate target.
    #[instrument(level = "debug", skip(target), err(level = "warn"))]
    pub(crate) fn for_target(target: &super::CrateTarget) -> CordialResult<Self> {
        let src_root = target.crate_root().join("src");
        let view = Self::new(
            target.crate_name().clone(),
            src_root.clone(),
            read_source_files(&src_root)?,
            target.kinds().clone(),
        )
        .with_binary_entries(target.binary_entries().clone())
        .with_manifest_snapshot(target.manifest_snapshot().clone());
        Ok(view)
    }
}

#[instrument(level = "debug", skip(src_root), err(level = "warn"))]
fn read_source_files(src_root: &Path) -> CordialResult<Vec<SourceFile>> {
    let mut files = Vec::new();
    if src_root.is_dir() {
        for entry in walkdir::WalkDir::new(src_root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            let source = super::source_work::read(path, SourceLoader::ID)?;
            files.push(SourceFile::new(path.to_path_buf(), source));
        }
    }
    Ok(files)
}
