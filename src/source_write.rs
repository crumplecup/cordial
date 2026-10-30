//! Shared staging, snapshot checks, and rollback for complete source edit units.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use tempfile::NamedTempFile;
use tracing::instrument;

use crate::error::{CordialError, CordialResult};

/// Transient transaction data, constructed only after an apply plan is accepted.
#[derive(derive_new::new)]
#[new(visibility = "pub(crate)")]
pub(crate) struct SourceWrite {
    path: PathBuf,
    source: String,
    replacement: String,
}

impl SourceWrite {
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn matches_disk(&self) -> CordialResult<bool> {
        Ok(fs::read(&self.path)? == self.source.as_bytes())
    }
}

struct StagedEdit<'a> {
    file: &'a SourceWrite,
    replacement: NamedTempFile,
    original: NamedTempFile,
}

#[instrument(level = "debug", skip(planned, validate), err(level = "warn"))]
pub(crate) fn write_sources(
    planned: &[SourceWrite],
    dry_run: bool,
    validate: impl Fn() -> CordialResult<()>,
) -> CordialResult<()> {
    validate()?;
    let mut paths = std::collections::HashSet::new();
    for file in planned {
        if !paths.insert(&file.path) {
            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
        }
        let metadata = fs::symlink_metadata(&file.path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(io::Error::from(io::ErrorKind::Unsupported).into());
        }
        if !file.matches_disk()? {
            return Err(CordialError::stale_source_snapshot(&file.path));
        }
    }
    if dry_run {
        return Ok(());
    }

    // Keep a replacement and an original in the source directory for every
    // file. A staging failure leaves all source paths untouched.
    let mut staged = Vec::with_capacity(planned.len());
    for file in planned {
        let metadata = fs::symlink_metadata(&file.path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(io::Error::from(io::ErrorKind::Unsupported).into());
        }
        let permissions = metadata.permissions();
        staged.push(StagedEdit {
            file,
            replacement: stage(&file.path, file.replacement.as_bytes(), &permissions)?,
            original: stage(&file.path, file.source.as_bytes(), &permissions)?,
        });
    }
    for item in &staged {
        if !item.file.matches_disk()? {
            return Err(CordialError::stale_source_snapshot(&item.file.path));
        }
    }

    let mut committed = Vec::new();
    for item in staged {
        if let Err(error) = validate() {
            restore(committed)?;
            return Err(error);
        }
        match item.file.matches_disk() {
            Ok(true) => {}
            Ok(false) => {
                restore(committed)?;
                return Err(CordialError::stale_source_snapshot(&item.file.path));
            }
            Err(error) => {
                restore(committed)?;
                return Err(error);
            }
        }
        let path = item.file.path.as_path();
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
