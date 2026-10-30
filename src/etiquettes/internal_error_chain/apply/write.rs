//! Route error field edits through the shared source transaction.

use crate::error::{CordialError, CordialResult};
use crate::loader::SourceEditView;
use crate::source_write::{SourceWrite, write_sources};

#[tracing::instrument(level = "debug", skip(planned), err(level = "warn"))]
pub(super) fn write_planned_files(
    planned: &[(&SourceEditView, String, usize)],
    dry_run: bool,
) -> CordialResult<()> {
    let mut writes = Vec::with_capacity(planned.len());
    for (file, replacement, _) in planned {
        if !file.matches_disk()? {
            return Err(CordialError::stale_source_snapshot(file.path()));
        }
        writes.push(SourceWrite::new(
            file.path().to_path_buf(),
            file.source().to_owned(),
            replacement.clone(),
        ));
    }
    write_sources(&writes, dry_run, || Ok(()))?;
    if dry_run {
        for (file, _, converted) in planned {
            tracing::info!(path = %file.path().display(), converted, "dry run: would update error field storage");
        }
    }
    Ok(())
}
