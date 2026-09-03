//! Insert missing crate-root inner attributes.
//!
//! `cordial quality --apply` scans each library root (no checklist) and
//! writes `#![forbid(unsafe_code)]` / `#![warn(missing_docs)]` when those
//! rules are armed and the file does not already satisfy them. `--dry-run`
//! logs without writing. `[lib] path`, bin-only packages, and
//! `[crate_attrs]` allow lists are the same as the scanner.

use std::path::Path;

use syn::spanned::Spanned;

use crate::config::load_cordial_config;
use crate::error::CordialResult;
use crate::session::RunAll;
use crate::targets::discover_crate_targets;

use super::scan::{library_root_rs, scan_crate_attrs};
use super::types::CrateAttrsRuleId;

use tracing::instrument;

/// Result of applying crate-root lint attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateAttrsApplySummary {
    /// Library root files written (or that would be written under dry-run).
    pub changed_files: usize,
    /// Individual `#![…]` lines inserted.
    pub inserted_attrs: usize,
    /// Library crates that already satisfied the armed rules (or were skipped
    /// by config).
    pub skipped_existing: usize,
    /// Library roots the scanner named that were missing or unparseable.
    pub unresolved: usize,
}

/// Patch library roots that are missing `forbid(unsafe_code)` / `warn(missing_docs)`.
#[instrument(level = "info", err(level = "warn"))]
pub fn run_crate_attrs_apply(
    project_root: &Path,
    store_home: &Path,
    only_crate: Option<&str>,
    dry_run: bool,
) -> CordialResult<CrateAttrsApplySummary> {
    let targets = discover_crate_targets(project_root, &RunAll)?;
    let policy = load_cordial_config(project_root, store_home)
        .crate_attrs()
        .clone();

    let mut summary = CrateAttrsApplySummary {
        changed_files: 0,
        inserted_attrs: 0,
        skipped_existing: 0,
        unresolved: 0,
    };

    for target in targets {
        let crate_name = target.crate_name();
        let crate_root = target.crate_root();
        if only_crate.is_some_and(|name| name != crate_name.as_str()) {
            continue;
        }
        if library_root_rs(crate_root).is_none() {
            continue;
        }
        let records = scan_crate_attrs(crate_root, crate_name, &policy)?;
        if records.is_empty() {
            summary.skipped_existing += 1;
            continue;
        }

        let Some(lib) = library_root_rs(crate_root) else {
            continue;
        };
        if !lib.is_file() {
            tracing::warn!(
                path = %lib.display(),
                crate_name = %crate_name,
                "library root does not exist"
            );
            summary.unresolved += records.len();
            continue;
        }

        let source = std::fs::read_to_string(&lib)?;
        let to_insert = attr_lines_for(&records);
        if to_insert.is_empty() {
            summary.skipped_existing += 1;
            continue;
        }
        let patched = match insert_crate_attrs(&source, &lib, &to_insert) {
            Ok(patched) => patched,
            Err(error) => {
                tracing::warn!(
                    path = %lib.display(),
                    crate_name = %crate_name,
                    error = %error,
                    "failed to insert crate attributes"
                );
                summary.unresolved += records.len();
                continue;
            }
        };
        if patched == source {
            summary.skipped_existing += 1;
            continue;
        }

        if dry_run {
            tracing::info!(path = %lib.display(), "dry run: would update file");
        } else {
            std::fs::write(&lib, patched)?;
        }
        summary.changed_files += 1;
        summary.inserted_attrs += to_insert.len();
    }

    tracing::info!(
        changed_files = summary.changed_files,
        inserted_attrs = summary.inserted_attrs,
        skipped_existing = summary.skipped_existing,
        unresolved = summary.unresolved,
        dry_run,
        "crate-attrs apply complete"
    );
    Ok(summary)
}

#[instrument(level = "debug", skip(records))]
fn attr_lines_for(records: &[super::types::CrateAttrsSiteRecord]) -> Vec<String> {
    let mut lines = Vec::new();
    if records
        .iter()
        .any(|record| record.rule_id() == CrateAttrsRuleId::ForbidUnsafe001)
    {
        lines.push("#![forbid(unsafe_code)]".to_string());
    }
    if records
        .iter()
        .any(|record| record.rule_id() == CrateAttrsRuleId::MissingDocs001)
    {
        lines.push("#![warn(missing_docs)]".to_string());
    }
    lines
}

#[instrument(level = "debug", skip(source, path, to_insert), err(level = "warn"))]
fn insert_crate_attrs(source: &str, path: &Path, to_insert: &[String]) -> CordialResult<String> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(path.display().to_string(), err))?;
    let mut lines: Vec<String> = source.lines().map(str::to_string).collect();
    let insert_at = match syntax.items.first() {
        Some(item) => item.span().start().line.saturating_sub(1).min(lines.len()),
        None => lines.len(),
    };
    for line in to_insert.iter().rev() {
        lines.insert(insert_at, line.clone());
    }
    let mut body = lines.join("\n");
    if source.ends_with('\n') || !body.ends_with('\n') {
        body.push('\n');
    }
    Ok(body)
}
