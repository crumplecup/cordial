//! Backup, load, and sync exception registry subtrees.

use std::fs;
use std::path::{Path, PathBuf};

use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::store::StoreLayout;

/// Backup curated exception files into `{backup_root}/{slug}/...`.
///
/// Writes `exceptions/`, `quality/patches/`, and `patches/` (coverage skip
/// lists). The last two keep elicit_doc's registry layout so a checkout like
/// elicitation's `.elicit_doc-exceptions/` loads without renaming.
#[instrument(level = "debug", skip(store))]
pub fn backup_exception_files(store: &StoreLayout, backup_root: &Path) -> CordialResult<usize> {
    store.ensure_dirs()?;
    let backup_slug_root = backup_root.join(store.project_slug());
    let mut copied = 0usize;
    for (relative, from) in exception_subtrees(store) {
        copied += sync_exception_subtree(&from, &backup_slug_root.join(relative))?;
    }
    prune_empty_dirs_up_to(&backup_slug_root, backup_root)?;
    Ok(copied)
}

/// Load curated exception files from `{backup_root}/{slug}/...` into the store.
///
/// Replaces the matching store subtrees. Missing backup subtrees clear the
/// corresponding store dirs so the registry is the source of truth.
#[instrument(level = "info", skip(store), err(level = "warn"))]
pub fn load_exception_files(store: &StoreLayout, backup_root: &Path) -> CordialResult<usize> {
    let backup_slug_root = backup_root.join(store.project_slug());
    if !backup_slug_root.is_dir() {
        return Err(CordialError::not_found(backup_slug_root));
    }
    store.ensure_dirs()?;
    let mut copied = 0usize;
    for (relative, to) in exception_subtrees(store) {
        copied += sync_exception_subtree(&backup_slug_root.join(relative), &to)?;
    }
    Ok(copied)
}

#[instrument(level = "trace", skip(store))]
fn exception_subtrees(store: &StoreLayout) -> [(&'static str, PathBuf); 3] {
    [
        ("exceptions", store.exceptions_dir()),
        ("quality/patches", store.quality_patches_dir()),
        ("patches", store.patches_dir()),
    ]
}

#[instrument(level = "debug", skip(from, to), err(level = "warn"))]
fn sync_exception_subtree(from: &Path, to: &Path) -> CordialResult<usize> {
    if to.exists() {
        fs::remove_dir_all(to)?;
    }
    if !from.exists() || (from.is_dir() && dir_is_empty(from)?) {
        return Ok(0);
    }
    fs::create_dir_all(to)?;
    copy_tree_overwrite(from, to)
}

#[instrument(level = "debug", skip(from, to), err(level = "warn"))]
fn copy_tree_overwrite(from: &Path, to: &Path) -> CordialResult<usize> {
    let mut copied = 0usize;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dest = to.join(entry.file_name());
        if src.is_dir() {
            fs::create_dir_all(&dest)?;
            copied += copy_tree_overwrite(&src, &dest)?;
        } else if src.is_file() {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&src, &dest)?;
            copied += 1;
        }
    }
    Ok(copied)
}

#[instrument(level = "debug", skip(start, stop), err(level = "warn"))]
fn prune_empty_dirs_up_to(start: &Path, stop: &Path) -> CordialResult<()> {
    let mut current = start.to_path_buf();
    while current.starts_with(stop) && current != stop {
        if !current.exists() || !dir_is_empty(&current)? {
            break;
        }
        fs::remove_dir(&current)?;
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent.to_path_buf();
    }
    Ok(())
}

#[instrument(level = "trace", skip(path), err(level = "warn"))]
fn dir_is_empty(path: &Path) -> CordialResult<bool> {
    Ok(fs::read_dir(path)?.next().transpose()?.is_none())
}
