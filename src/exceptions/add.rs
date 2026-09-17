//! Append new quality exceptions and coverage skip rows.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::store::StoreLayout;

use super::entry::ExceptionEntry;
use super::json::{
    json_rows_contain_path, parse_exception_file, parse_json_array, write_pretty_json,
};
use super::paths::{coverage_skip_file_path, exception_file_path};

/// One coverage skip-list row in `{store}/patches/{patch_set}.json`.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct CoverageSkipEntry {
    /// Qualified path or type name to skip.
    #[new(into)]
    path: String,
    /// Human-readable explanation shown in reports.
    #[new(into)]
    reason: String,
}

/// Result of appending one exception row to a store file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddExceptionOutcome {
    /// The row was written to `path`.
    Inserted {
        /// Store file that received the new row.
        path: PathBuf,
    },
    /// An identical row was already present at `path`.
    AlreadyPresent {
        /// Store file that already contained the row.
        path: PathBuf,
    },
}

impl AddExceptionOutcome {
    /// Store file this outcome refers to.
    #[instrument(level = "trace", skip(self))]
    pub fn path(&self) -> &Path {
        match self {
            Self::Inserted { path } | Self::AlreadyPresent { path } => path,
        }
    }

    /// Inserted.
    #[instrument(level = "trace", skip(self))]
    pub fn inserted(&self) -> bool {
        matches!(self, Self::Inserted { .. })
    }
}

/// Append a quality exception to `{store}/exceptions/{etiquette}/{crate}.json`.
///
/// Creates the file when missing. An identical row is a no-op. Rows that
/// already live in the elicit_doc alias are left there and not duplicated.
#[instrument(level = "debug", skip(store, entry))]
pub fn add_exception(
    store: &StoreLayout,
    etiquette_id: &str,
    crate_name: &str,
    entry: ExceptionEntry,
) -> CordialResult<AddExceptionOutcome> {
    require_nonempty("etiquette", etiquette_id)?;
    require_nonempty("crate_name", crate_name)?;
    let entry = entry.normalized_for_store()?;
    let canonical = exception_file_path(store, etiquette_id, crate_name);
    let alias = store
        .quality_patches_dir()
        .join(etiquette_id)
        .join(format!("{crate_name}.json"));

    let mut entries = if canonical.is_file() {
        parse_exception_file(&canonical)?
    } else {
        Vec::new()
    };
    if entries.contains(&entry) {
        return Ok(AddExceptionOutcome::AlreadyPresent { path: canonical });
    }
    if alias.is_file() {
        let alias_entries = parse_exception_file(&alias)?;
        if alias_entries.contains(&entry) {
            return Ok(AddExceptionOutcome::AlreadyPresent { path: alias });
        }
    }

    entries.push(entry);
    write_pretty_json(&canonical, &entries)?;
    Ok(AddExceptionOutcome::Inserted { path: canonical })
}

/// Append a coverage skip to `{store}/patches/{patch_set}.json`.
///
/// Existing objects keep unknown fields (for example `verifiers`). An
/// identical `path` in the canonical file or the `exceptions/` alias is a
/// no-op.
#[instrument(level = "debug", skip(store, entry))]
pub fn add_coverage_skip(
    store: &StoreLayout,
    patch_set: &str,
    entry: CoverageSkipEntry,
) -> CordialResult<AddExceptionOutcome> {
    require_nonempty("patch_set", patch_set)?;
    let entry = normalize_coverage_entry(entry)?;
    let canonical = coverage_skip_file_path(store, patch_set);
    let alias = store.exceptions_dir().join(format!("{patch_set}.json"));

    let mut rows = if canonical.is_file() {
        parse_json_array(&canonical)?
    } else {
        Vec::new()
    };
    if json_rows_contain_path(&rows, &entry.path) {
        return Ok(AddExceptionOutcome::AlreadyPresent { path: canonical });
    }
    if alias.is_file() && json_rows_contain_path(&parse_json_array(&alias)?, &entry.path) {
        return Ok(AddExceptionOutcome::AlreadyPresent { path: alias });
    }

    rows.push(serde_json::to_value(&entry)?);
    write_pretty_json(&canonical, &rows)?;
    Ok(AddExceptionOutcome::Inserted { path: canonical })
}

#[instrument(level = "debug")]
fn require_nonempty(label: &str, value: &str) -> CordialResult<()> {
    if value.trim().is_empty() {
        return Err(CordialError::invariant(format!(
            "{label} must not be empty"
        )));
    }
    Ok(())
}

#[instrument(level = "debug", skip(entry), err(level = "warn"))]
fn normalize_coverage_entry(mut entry: CoverageSkipEntry) -> CordialResult<CoverageSkipEntry> {
    entry.path = entry.path.trim().to_string();
    entry.reason = entry.reason.trim().to_string();
    require_nonempty("path", &entry.path)?;
    require_nonempty("reason", &entry.reason)?;
    Ok(entry)
}
