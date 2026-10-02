//! Remove or correct one quality exception row addressed by a unique selector.

use std::fs;
use std::path::{Path, PathBuf};

use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::store::StoreLayout;

use super::entry::ExceptionEntry;
use super::json::{parse_exception_file, write_pretty_json};
use super::paths::{exception_file_path, normalize_rel_path};

/// Names one exception row by any combination of its fields.
///
/// Every field that is set must equal the stored value exactly. At least one
/// field is required, and the selector must identify exactly one row.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, derive_getters::Getters, derive_setters::Setters,
)]
#[setters(prefix = "with_", strip_option, into)]
pub struct ExceptionSelector {
    /// Source file relative to the crate root.
    file: Option<String>,
    /// Line number on the stored row.
    #[getter(copy)]
    line: Option<u32>,
    /// Rule id on the stored row.
    rule_id: Option<String>,
    /// Context / qualified name on the stored row.
    context: Option<String>,
}

impl ExceptionSelector {
    /// Whether no field is set.
    #[instrument(level = "trace", skip(self))]
    pub fn is_empty(&self) -> bool {
        self.file.is_none()
            && self.line.is_none()
            && self.rule_id.is_none()
            && self.context.is_none()
    }

    #[instrument(level = "trace", skip(self, entry))]
    fn selects(&self, entry: &ExceptionEntry) -> bool {
        let file_ok = self
            .file
            .as_deref()
            .is_none_or(|file| normalize_rel_path(Path::new(file.trim())) == *entry.file());
        let line_ok = self.line.is_none_or(|line| entry.line() == Some(line));
        let rule_ok = self
            .rule_id
            .as_deref()
            .is_none_or(|rule| entry.rule_id().as_deref() == Some(rule.trim()));
        let context_ok = self
            .context
            .as_deref()
            .is_none_or(|context| entry.context().as_deref() == Some(context.trim()));
        file_ok && line_ok && rule_ok && context_ok
    }
}

/// New values for an existing row. Unset fields keep their stored value.
///
/// An empty `rule_id` or `context` clears that field, matching how `add`
/// normalizes blank values.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, derive_getters::Getters, derive_setters::Setters,
)]
#[setters(prefix = "with_", strip_option, into)]
pub struct ExceptionUpdate {
    /// Replacement source file.
    file: Option<String>,
    /// Replacement line number.
    #[getter(copy)]
    line: Option<u32>,
    /// Replacement rule id.
    rule_id: Option<String>,
    /// Replacement context / qualified name.
    context: Option<String>,
    /// Replacement reason.
    reason: Option<String>,
}

impl ExceptionUpdate {
    /// Whether no field is set.
    #[instrument(level = "trace", skip(self))]
    pub fn is_empty(&self) -> bool {
        self.file.is_none()
            && self.line.is_none()
            && self.rule_id.is_none()
            && self.context.is_none()
            && self.reason.is_none()
    }

    #[instrument(level = "debug", skip(self, entry), err(level = "warn"))]
    fn apply_to(&self, entry: &ExceptionEntry) -> CordialResult<ExceptionEntry> {
        let file = self.file.clone().unwrap_or_else(|| entry.file().clone());
        let reason = self
            .reason
            .clone()
            .unwrap_or_else(|| entry.reason().clone());
        let mut next = ExceptionEntry::new(file, reason);
        if let Some(line) = self.line.or(entry.line()) {
            next = next.with_line(line);
        }
        if let Some(rule_id) = self.rule_id.clone().or_else(|| entry.rule_id().clone()) {
            next = next.with_rule_id(rule_id);
        }
        if let Some(context) = self.context.clone().or_else(|| entry.context().clone()) {
            next = next.with_context(context);
        }
        next.normalized_for_store()
    }
}

/// Result of removing one exception row.
#[derive(Debug, Clone, PartialEq, Eq, derive_new::new, derive_getters::Getters)]
pub struct RemoveExceptionOutcome {
    /// Store file the row was removed from.
    path: PathBuf,
    /// The row that was removed.
    removed: ExceptionEntry,
    /// Whether removing the last row deleted the file.
    #[getter(copy)]
    file_deleted: bool,
}

/// Result of editing one exception row in place.
#[derive(Debug, Clone, PartialEq, Eq, derive_new::new, derive_getters::Getters)]
pub struct UpdateExceptionOutcome {
    /// Store file that holds the row.
    path: PathBuf,
    /// The row before the edit.
    before: ExceptionEntry,
    /// The row after the edit.
    after: ExceptionEntry,
}

struct ExceptionFile {
    path: PathBuf,
    entries: Vec<ExceptionEntry>,
}

struct Located {
    files: Vec<ExceptionFile>,
    file: usize,
    row: usize,
}

/// Delete exactly one row from `{store}/exceptions/{etiquette}/{crate}.json`
/// (or its elicit_doc alias).
///
/// Errors when the selector is empty, matches no row, or matches several. A
/// file left with no rows is deleted rather than kept as an empty array.
#[instrument(level = "debug", skip(store, selector), err(level = "warn"))]
pub fn remove_exception(
    store: &StoreLayout,
    etiquette_id: &str,
    crate_name: &str,
    selector: &ExceptionSelector,
) -> CordialResult<RemoveExceptionOutcome> {
    let Located {
        mut files,
        file,
        row,
    } = locate(store, etiquette_id, crate_name, selector)?;
    let target = &mut files[file];
    let removed = target.entries.remove(row);
    let file_deleted = target.entries.is_empty();
    if file_deleted {
        delete_file_and_empty_parent(&target.path)?;
    } else {
        write_pretty_json(&target.path, &target.entries)?;
    }
    Ok(RemoveExceptionOutcome::new(
        files.swap_remove(file).path,
        removed,
        file_deleted,
    ))
}

/// Replace exactly one row in place, keeping its position in the file.
///
/// Errors when the selector is empty, matches no row or several, the update
/// sets nothing, or the edited row would duplicate another row.
#[instrument(level = "debug", skip(store, selector, update), err(level = "warn"))]
pub fn update_exception(
    store: &StoreLayout,
    etiquette_id: &str,
    crate_name: &str,
    selector: &ExceptionSelector,
    update: &ExceptionUpdate,
) -> CordialResult<UpdateExceptionOutcome> {
    if update.is_empty() {
        return Err(CordialError::invariant(
            "edit requires at least one new value (--new-file, --new-line, \
             --new-rule-id, --new-context, or --new-reason)",
        ));
    }
    let Located {
        mut files,
        file,
        row,
    } = locate(store, etiquette_id, crate_name, selector)?;
    let before = files[file].entries[row].clone();
    let after = update.apply_to(&before)?;
    let duplicate = files.iter().enumerate().any(|(file_index, candidate)| {
        candidate
            .entries
            .iter()
            .enumerate()
            .any(|(row_index, entry)| (file_index, row_index) != (file, row) && *entry == after)
    });
    let rendered = serde_json::to_string(&after)?;
    if duplicate {
        return Err(CordialError::invariant(format!(
            "edited exception duplicates an existing row: {rendered}"
        )));
    }
    let target = &mut files[file];
    target.entries[row] = after.clone();
    write_pretty_json(&target.path, &target.entries)?;
    Ok(UpdateExceptionOutcome::new(
        files.swap_remove(file).path,
        before,
        after,
    ))
}

#[instrument(level = "debug", skip(store, selector), err(level = "warn"))]
fn locate(
    store: &StoreLayout,
    etiquette_id: &str,
    crate_name: &str,
    selector: &ExceptionSelector,
) -> CordialResult<Located> {
    if etiquette_id.trim().is_empty() {
        return Err(CordialError::invariant("etiquette must not be empty"));
    }
    if crate_name.trim().is_empty() {
        return Err(CordialError::invariant("crate_name must not be empty"));
    }
    if selector.is_empty() {
        return Err(CordialError::invariant(
            "selector requires at least one of --rule-id, --context, --file, --line",
        ));
    }
    let canonical = exception_file_path(store, etiquette_id, crate_name);
    let alias = store
        .quality_patches_dir()
        .join(etiquette_id)
        .join(format!("{crate_name}.json"));
    let mut files = Vec::new();
    for path in [canonical, alias] {
        if path.is_file() {
            let entries = parse_exception_file(&path)?;
            files.push(ExceptionFile { path, entries });
        }
    }
    if files.is_empty() {
        return Err(CordialError::no_exceptions(etiquette_id, crate_name));
    }

    let hits: Vec<(usize, usize)> = files
        .iter()
        .enumerate()
        .flat_map(|(file, candidate)| {
            candidate
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| selector.selects(entry))
                .map(move |(row, _)| (file, row))
        })
        .collect();
    match hits.as_slice() {
        [(file, row)] => Ok(Located {
            file: *file,
            row: *row,
            files,
        }),
        [] => Err(CordialError::invariant(format!(
            "no exception for etiquette `{etiquette_id}` crate `{crate_name}` matches {selector:?}"
        ))),
        many => {
            let mut listing = String::new();
            for (file, row) in many {
                listing.push_str("\n  ");
                listing.push_str(&serde_json::to_string(&files[*file].entries[*row])?);
            }
            Err(CordialError::invariant(format!(
                "{} exceptions for etiquette `{etiquette_id}` crate `{crate_name}` match \
                 {selector:?}; add more selector fields to pick one:{listing}",
                many.len()
            )))
        }
    }
}

#[instrument(level = "debug", skip(path), err(level = "warn"))]
fn delete_file_and_empty_parent(path: &Path) -> CordialResult<()> {
    fs::remove_file(path)?;
    if let Some(parent) = path.parent()
        && fs::read_dir(parent)?.next().transpose()?.is_none()
    {
        fs::remove_dir(parent)?;
    }
    Ok(())
}
