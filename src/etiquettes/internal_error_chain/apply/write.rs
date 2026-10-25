//! Stage a crate target's apply edits before replacing any source file.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

use tempfile::NamedTempFile;
use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::loader::SourceEditView;

struct StagedEdit<'a> {
    file: &'a SourceEditView,
    replacement: NamedTempFile,
    original: NamedTempFile,
}

#[instrument(level = "debug", skip(planned), err(level = "warn"))]
pub(super) fn write_planned_files(
    planned: &[(&SourceEditView, String, usize)],
    dry_run: bool,
) -> CordialResult<()> {
    for (file, _, _) in planned {
        if !file.matches_disk()? {
            return Err(CordialError::stale_source_snapshot(file.path()));
        }
    }
    if dry_run {
        for (file, _, converted) in planned {
            tracing::info!(path = %file.path().display(), converted, "dry run: would update error field storage");
        }
        return Ok(());
    }

    // Keep a replacement and an original in the source directory for every
    // file. A staging failure leaves all source paths untouched.
    let mut staged = Vec::with_capacity(planned.len());
    for (file, patched, _) in planned {
        let metadata = fs::symlink_metadata(file.path())?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(io::Error::from(io::ErrorKind::Unsupported).into());
        }
        let permissions = metadata.permissions();
        staged.push(StagedEdit {
            file,
            replacement: stage(file.path(), patched.as_bytes(), &permissions)?,
            original: stage(file.path(), file.source().as_bytes(), &permissions)?,
        });
    }
    for item in &staged {
        if !item.file.matches_disk()? {
            return Err(CordialError::stale_source_snapshot(item.file.path()));
        }
    }

    let mut committed = Vec::new();
    for item in staged {
        match item.file.matches_disk() {
            Ok(true) => {}
            Ok(false) => {
                restore(committed)?;
                return Err(CordialError::stale_source_snapshot(item.file.path()));
            }
            Err(error) => {
                restore(committed)?;
                return Err(error);
            }
        }
        let path = item.file.path();
        if let Err(error) = item.replacement.persist(path) {
            restore(committed)?;
            return Err(error.error.into());
        }
        committed.push((path, item.original));
    }
    Ok(())
}

#[instrument(
    level = "debug",
    skip(path, contents, permissions),
    err(level = "warn")
)]
fn stage(path: &Path, contents: &[u8], permissions: &fs::Permissions) -> io::Result<NamedTempFile> {
    let parent = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    let mut staged = NamedTempFile::new_in(parent)?;
    staged.write_all(contents)?;
    staged.as_file().set_permissions(permissions.clone())?;
    Ok(staged)
}

#[instrument(level = "debug", skip(committed), err(level = "warn"))]
fn restore(committed: Vec<(&Path, NamedTempFile)>) -> CordialResult<()> {
    let mut first_error = None;
    for (path, original) in committed.into_iter().rev() {
        if let Err(error) = original.persist(path) {
            tracing::error!(path = %path.display(), source = %error.error, "could not restore source after apply write failure");
            if let Err(keep_error) = error.file.keep() {
                tracing::error!(path = %path.display(), source = %keep_error.error, "could not retain original source backup");
            }
            first_error.get_or_insert(error.error);
        }
    }
    if let Some(error) = first_error {
        return Err(error.into());
    }
    Ok(())
}
