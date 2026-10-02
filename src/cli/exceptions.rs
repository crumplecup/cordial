//! `cordial exceptions` command bodies. Clap types in `cli::commands` call these after `act` dispatch.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::{
    AddExceptionOutcome, CordialError, CordialResult, CoverageSkipEntry, ExceptionEntry,
    ExceptionSelector, ExceptionUpdate, StoreLayout, add_coverage_skip, add_exception,
    backup_exception_files, load_exception_files, load_exceptions, remove_exception,
    resolve_exceptions_root, update_exception,
};
use tracing::instrument;

#[instrument(level = "debug", skip(store), err(level = "warn"))]
pub(super) fn execute_backup_exceptions(
    project_root: &Path,
    store: &StoreLayout,
    root: &Path,
) -> CordialResult<()> {
    let backup_root = resolve_exceptions_root(project_root, root);
    let copied = backup_exception_files(store, &backup_root)?;
    tracing::info!(
        copied,
        path = %backup_root.join(store.project_slug()).display(),
        "backed up exception files"
    );
    Ok(())
}

#[instrument(level = "debug", skip(store), err(level = "warn"))]
pub(super) fn execute_load_exceptions(
    project_root: &Path,
    store: &StoreLayout,
    root: &Path,
) -> CordialResult<()> {
    let backup_root = resolve_exceptions_root(project_root, root);
    let copied = load_exception_files(store, &backup_root)?;
    tracing::info!(
        copied,
        path = %backup_root.join(store.project_slug()).display(),
        "loaded exception files"
    );
    Ok(())
}

#[instrument(level = "debug", skip(store, entry), err(level = "warn"))]
pub(super) fn execute_add_exception(
    store: &StoreLayout,
    etiquette: &str,
    crate_name: &str,
    entry: ExceptionEntry,
) -> CordialResult<()> {
    print_add_outcome(add_exception(store, etiquette, crate_name, entry)?)
}

#[instrument(level = "debug", skip(store, selector), err(level = "warn"))]
pub(super) fn execute_remove_exception(
    store: &StoreLayout,
    etiquette: &str,
    crate_name: &str,
    selector: &ExceptionSelector,
) -> CordialResult<()> {
    let outcome = remove_exception(store, etiquette, crate_name, selector)?;
    tracing::info!(
        path = %outcome.path().display(),
        file_deleted = outcome.file_deleted(),
        removed = %serde_json::to_string(outcome.removed())?,
        "exception row removed"
    );
    Ok(())
}

#[instrument(level = "debug", skip(store, selector, update), err(level = "warn"))]
pub(super) fn execute_edit_exception(
    store: &StoreLayout,
    etiquette: &str,
    crate_name: &str,
    selector: &ExceptionSelector,
    update: &ExceptionUpdate,
) -> CordialResult<()> {
    let outcome = update_exception(store, etiquette, crate_name, selector, update)?;
    tracing::info!(
        path = %outcome.path().display(),
        before = %serde_json::to_string(outcome.before())?,
        after = %serde_json::to_string(outcome.after())?,
        "exception row edited"
    );
    Ok(())
}

#[instrument(level = "debug", skip(store, entry), err(level = "warn"))]
pub(super) fn execute_add_coverage_skip(
    store: &StoreLayout,
    patch_set: &str,
    entry: CoverageSkipEntry,
) -> CordialResult<()> {
    print_add_outcome(add_coverage_skip(store, patch_set, entry)?)
}

#[instrument(level = "debug", skip(outcome))]
fn print_add_outcome(outcome: AddExceptionOutcome) -> CordialResult<()> {
    let verb = if outcome.inserted() {
        "added"
    } else {
        "already present"
    };
    tracing::info!(verb, path = %outcome.path().display(), "exception row");
    Ok(())
}

#[instrument(level = "debug", skip(store), err(level = "warn"))]
pub(super) fn list_exceptions(store: &StoreLayout) -> CordialResult<()> {
    let mut files = Vec::new();
    let exceptions_dir = store.exceptions_dir();
    if exceptions_dir.is_dir() {
        collect_files(&exceptions_dir, &exceptions_dir, &mut files, "exceptions")?;
    }
    let quality_patches_dir = store.quality_patches_dir();
    if quality_patches_dir.is_dir() {
        collect_files(
            &quality_patches_dir,
            &quality_patches_dir,
            &mut files,
            "quality/patches",
        )?;
    }
    let patches_dir = store.patches_dir();
    if patches_dir.is_dir() {
        collect_files(&patches_dir, &patches_dir, &mut files, "patches")?;
    }
    files.sort();
    if files.is_empty() {
        tracing::warn!(
            exceptions = %exceptions_dir.display(),
            quality_patches = %quality_patches_dir.display(),
            patches = %patches_dir.display(),
            "no exception files"
        );
        return Ok(());
    }
    for path in files {
        writeln!(io::stdout(), "{}", path.display())?;
    }
    Ok(())
}

#[instrument(level = "debug", err(level = "warn"))]
fn collect_files(
    base: &Path,
    current: &Path,
    out: &mut Vec<PathBuf>,
    prefix: &str,
) -> CordialResult<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(base, &path, out, prefix)?;
        } else if path.extension().is_some_and(|ext| ext == "json") {
            let rel = path.strip_prefix(base)?;
            out.push(PathBuf::from(prefix).join(rel));
        }
    }
    Ok(())
}

#[instrument(level = "debug", skip(store), err(level = "warn"))]
pub(super) fn show_exceptions(
    store: &StoreLayout,
    etiquette: &str,
    crate_name: &str,
) -> CordialResult<()> {
    let set = load_exceptions(store, etiquette, crate_name)?;
    if set.is_empty() {
        return Err(CordialError::no_exceptions(etiquette, crate_name));
    }
    let file_name = format!("{crate_name}.json");
    let canonical = store.exceptions_dir().join(etiquette).join(&file_name);
    let alias = store.quality_patches_dir().join(etiquette).join(&file_name);
    let path = if canonical.is_file() {
        canonical
    } else {
        alias
    };
    let bytes = fs::read(&path)?;
    io::stdout().write_all(&bytes)?;
    if !bytes.ends_with(b"\n") {
        writeln!(io::stdout())?;
    }
    Ok(())
}
